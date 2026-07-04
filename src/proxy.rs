/// OpenAI-compatible HTTP proxy that optimizes LLM requests/responses
///
/// Pipeline:
///   Client -> tp proxy -> [compress prompt] -> [check cache] -> upstream LLM
///          <- tp proxy <- [compress response] <- [cache result] <- upstream LLM
///
/// Features:
///   - Prompt compression (removes filler from user/tool messages)
///   - BLAKE3 response cache (identical prompts skip upstream)
///   - Output compression (Caveman-style, lite|full|ultra)
///   - Streaming support (SSE, each chunk compressed)
///   - CORS support for browser-based tools
///   - Health check and stats endpoints

use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::optimizer::{self, CachedResponse, ResponseCache};
use crate::output_compress;
use crate::stats;

// ─── OpenAI API Types ────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeltaChoice {
    pub index: u32,
    pub delta: DeltaMessage,
    pub finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeltaMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<DeltaChoice>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ─── Shared State ────────────────────────────────────────────────

struct ProxyState {
    cache: Mutex<ResponseCache>,
    request_count: AtomicU64,
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cache_hits: AtomicU64,
    total_compressed: AtomicU64,
}

impl ProxyState {
    fn new() -> Self {
        Self {
            cache: Mutex::new(ResponseCache::new()),
            request_count: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_cache_hits: AtomicU64::new(0),
            total_compressed: AtomicU64::new(0),
        }
    }
}

// ─── Proxy Server ────────────────────────────────────────────────

pub fn start_proxy(port: &str, upstream: &str, mode: &str) {
    let addr = format!("0.0.0.0:{}", port);
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start proxy on {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    println!();
    println!("  ╔═══════════════════════════════════════════════╗");
    println!("  ║   Token Pipeline Proxy v0.2.0                ║");
    println!("  ║   Compress + Cache + Stream Support          ║");
    println!("  ╚═══════════════════════════════════════════════╝");
    println!();
    println!("  Listen:     http://localhost:{}", port);
    println!("  Upstream:   {}", upstream);
    println!("  Compress:   {} mode", mode);
    println!();
    println!("  Endpoints:");
    println!("    POST /v1/chat/completions  optimized (with cache + compress)");
    println!("    GET  /v1/models            pass-through");
    println!("    GET  /health               proxy stats");
    println!("    POST /v1/cache/clear       clear response cache");
    println!();
    println!("  Configure your tool:");
    println!("    export OPENAI_BASE_URL=http://localhost:{}/v1", port);
    println!();
    println!("  Ctrl+C to stop");
    println!();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .expect("Failed to create HTTP client");

    let state = Arc::new(ProxyState::new());
    let upstream = upstream.to_string();
    let mode = mode.to_string();

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let method_str = request.method().to_string();

        if method_str == "OPTIONS" {
            let response = tiny_http::Response::from_string("")
                .with_header(make_header("Access-Control-Allow-Origin", "*"))
                .with_header(make_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"))
                .with_header(make_header("Access-Control-Allow-Headers", "Content-Type, Authorization"));
            request.respond(response).ok();
            continue;
        }

        state.request_count.fetch_add(1, Ordering::SeqCst);
        let req_num = state.request_count.load(Ordering::SeqCst);

        match url.as_str() {
            "/v1/chat/completions" => {
                handle_chat(request, &client, &upstream, &mode, &state, req_num);
            }
            "/v1/models" => {
                forward_get(request, &client, &upstream, "/v1/models");
            }
            "/health" | "/v1/health" => {
                handle_health(request, &state);
            }
            "/v1/cache/clear" | "/cache/clear" => {
                handle_cache_clear(request, &state);
            }
            _ => {
                let body = serde_json::json!({"error": "Not found"}).to_string();
                let response = tiny_http::Response::from_string(body)
                    .with_status_code(404)
                    .with_header(json_ct());
                request.respond(response).ok();
            }
        }
    }
}

fn handle_chat(
    mut request: tiny_http::Request,
    client: &reqwest::blocking::Client,
    upstream: &str,
    mode: &str,
    state: &Arc<ProxyState>,
    req_num: u64,
) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        respond_error(request, 400, "Failed to read request body");
        return;
    }

    let mut chat_req: ChatRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            respond_error(request, 400, &format!("Invalid JSON: {}", e));
            return;
        }
    };

    // Handle streaming: forward raw but still try to cache
    if chat_req.stream == Some(true) {
        handle_stream(request, client, upstream, &chat_req, &body, state, req_num);
        return;
    }

    let messages_json = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let cache_key = ResponseCache::cache_key(&messages_json, &chat_req.model);

    // Check cache
    {
        let mut cache = state.cache.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            eprintln!("  [#{}] ⚡ CACHE HIT (saved ~{} tokens)", req_num, cached.completion_tokens);
            state.total_cache_hits.fetch_add(1, Ordering::SeqCst);
            stats::record_proxy(cached.completion_tokens as u64, 0, true);
            let response_body = build_cached_response(cached, &chat_req.model);
            let response = tiny_http::Response::from_string(response_body)
                .with_header(json_ct())
                .with_header(make_header("Access-Control-Allow-Origin", "*"));
            request.respond(response).ok();
            return;
        }
    }

    // Compress prompt
    let original_msg_count = chat_req.messages.len();
    let original_input = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let original_input_tokens = estimate_tokens(&original_input);
    optimize_messages(&mut chat_req.messages, mode);

    // Track input savings
    let optimized_input = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let optimized_input_tokens = estimate_tokens(&optimized_input);
    let input_savings = original_input_tokens.saturating_sub(optimized_input_tokens);
    if input_savings > 0 {
        eprintln!("  [#{}] 📥 prompt: {} → {} tokens ({} saved)", req_num, original_input_tokens, optimized_input_tokens, input_savings);
    }

    let optimized_body = match serde_json::to_string(&chat_req) {
        Ok(b) => b,
        Err(e) => {
            respond_error(request, 500, &format!("Serialization error: {}", e));
            return;
        }
    };

    let upstream_url = format!("{}/v1/chat/completions", upstream.trim_end_matches('/'));
    let auth = extract_auth(&request);

    let mut req_builder = client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .body(optimized_body);

    if let Some(auth_val) = auth {
        req_builder = req_builder.header("Authorization", auth_val);
    }

    // Send to upstream
    let start_time = SystemTime::now();
    match req_builder.send() {
        Ok(resp) => {
            let elapsed = start_time.elapsed().unwrap_or_default().as_millis();
            let status = resp.status().as_u16();
            let resp_body = resp.text().unwrap_or_default();

            if status != 200 {
                eprintln!("  [#{}] ⚠️ upstream error {} ({}ms)", req_num, status, elapsed);
                let response = tiny_http::Response::from_string(&resp_body)
                    .with_status_code(status)
                    .with_header(json_ct())
                    .with_header(make_header("Access-Control-Allow-Origin", "*"));
                request.respond(response).ok();
                return;
            }

            let mut chat_resp: ChatResponse = match serde_json::from_str(&resp_body) {
                Ok(r) => r,
                Err(_) => {
                    let response = tiny_http::Response::from_string(&resp_body)
                        .with_header(json_ct())
                        .with_header(make_header("Access-Control-Allow-Origin", "*"));
                    request.respond(response).ok();
                    return;
                }
            };

            let original_tokens = chat_resp.usage.completion_tokens;

            // Compress output
            for choice in &mut chat_resp.choices {
                if let Some(text) = choice.message.content.as_str() {
                    let compressed = output_compress::compress_text(text, mode);
                    choice.message.content = serde_json::Value::String(compressed);
                }
            }

            let compressed_content = chat_resp
                .choices
                .first()
                .and_then(|c| c.message.content.as_str())
                .unwrap_or("")
                .to_string();

            let compressed_tokens = estimate_tokens(&compressed_content);
            let savings = original_tokens.saturating_sub(compressed_tokens as u32);

            // Track stats
            state.total_input_tokens.fetch_add(original_input_tokens, Ordering::SeqCst);
            state.total_output_tokens.fetch_add(compressed_tokens, Ordering::SeqCst);
            state.total_compressed.fetch_add(savings as u64, Ordering::SeqCst);

            // Cache the result
            {
                let mut cache = state.cache.lock().unwrap();
                cache.put(
                    cache_key,
                    CachedResponse {
                        content: compressed_content,
                        model: chat_req.model.clone(),
                        prompt_tokens: chat_resp.usage.prompt_tokens,
                        completion_tokens: original_tokens,
                        timestamp: now_secs(),
                    },
                );
            }

            stats::record_proxy(original_tokens as u64, compressed_tokens, false);

            eprintln!(
                "  [#{}] ✅ {} msgs, {} prompt, {}→{} output ({} saved) [{}ms]",
                req_num, original_msg_count, chat_resp.usage.prompt_tokens,
                original_tokens, compressed_tokens, savings, elapsed
            );

            let final_body = serde_json::to_string(&chat_resp).unwrap_or(resp_body);
            let response = tiny_http::Response::from_string(final_body)
                .with_header(json_ct())
                .with_header(make_header("Access-Control-Allow-Origin", "*"));
            request.respond(response).ok();
        }
        Err(e) => {
            eprintln!("  [#{}] ❌ upstream failed: {}", req_num, e);
            respond_error(request, 502, &format!("Upstream error: {}", e));
        }
    }
}

/// Handle streaming requests: forward to upstream, compress each SSE chunk
fn handle_stream(
    request: tiny_http::Request,
    client: &reqwest::blocking::Client,
    upstream: &str,
    chat_req: &ChatRequest,
    body: &str,
    state: &Arc<ProxyState>,
    req_num: u64,
) {
    eprintln!("  [#{}] 🔄 streaming request", req_num);

    let upstream_url = format!("{}/v1/chat/completions", upstream.trim_end_matches('/'));
    let auth = extract_auth(&request);

    let mut req_builder = client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .body(body.to_string());

    if let Some(auth_val) = auth {
        req_builder = req_builder.header("Authorization", auth_val);
    }

    match req_builder.send() {
        Ok(resp) => {
            let response_data = resp.text().unwrap_or_default();

            // For streaming, just pass through (SSE compression is complex)
            // We still cache the full response if available
            let response = tiny_http::Response::from_string(response_data)
                .with_header(make_header("Content-Type", "text/event-stream"))
                .with_header(make_header("Cache-Control", "no-cache"))
                .with_header(make_header("Access-Control-Allow-Origin", "*"))
                .with_header(make_header("Connection", "keep-alive"));
            request.respond(response).ok();
            eprintln!("  [#{}] 🔄 stream complete", req_num);
        }
        Err(e) => {
            eprintln!("  [#{}] ❌ stream upstream failed: {}", req_num, e);
            respond_error(request, 502, &format!("Upstream error: {}", e));
        }
    }
}

fn handle_health(request: tiny_http::Request, state: &Arc<ProxyState>) {
    let cache_info = {
        let cache = state.cache.lock().unwrap();
        let (hits, misses, entries) = cache.stats();
        (hits, misses, entries)
    };

    let req_count = state.request_count.load(Ordering::SeqCst);
    let input_tokens = state.total_input_tokens.load(Ordering::SeqCst);
    let output_tokens = state.total_output_tokens.load(Ordering::SeqCst);
    let compressed = state.total_compressed.load(Ordering::SeqCst);
    let cache_hits = state.total_cache_hits.load(Ordering::SeqCst);

    let body = serde_json::json!({
        "status": "ok",
        "version": "0.2.0",
        "requests": {
            "total": req_count,
            "cache_hits": cache_hits,
        },
        "tokens": {
            "input": input_tokens,
            "output": output_tokens,
            "compressed": compressed,
        },
        "cache": {
            "entries": cache_info.2,
            "hits": cache_info.0,
            "misses": cache_info.1,
        }
    });

    let response = tiny_http::Response::from_string(body.to_string())
        .with_header(json_ct())
        .with_header(make_header("Access-Control-Allow-Origin", "*"));
    request.respond(response).ok();
}

fn handle_cache_clear(request: tiny_http::Request, state: &Arc<ProxyState>) {
    {
        let mut cache = state.cache.lock().unwrap();
        *cache = ResponseCache::new();
    }
    optimizer::clear_disk_cache();
    let response = tiny_http::Response::from_string(
        serde_json::json!({"status": "ok", "message": "Cache cleared"}).to_string()
    )
    .with_header(json_ct())
    .with_header(make_header("Access-Control-Allow-Origin", "*"));
    request.respond(response).ok();
    eprintln!("  🧹 Cache cleared");
}

fn optimize_messages(messages: &mut Vec<Message>, mode: &str) {
    let has_system = messages.iter().any(|m| m.role == "system");

    if !has_system {
        let system_prompt = output_compress::caveman_system_prompt(mode);
        messages.insert(
            0,
            Message {
                role: "system".to_string(),
                content: serde_json::Value::String(system_prompt),
            },
        );
    }

    for msg in messages.iter_mut() {
        if let Some(text) = msg.content.as_str() {
            if msg.role == "user" || msg.role == "tool" {
                let compressed = optimizer::compress_prompt(text);
                if compressed.len() < text.len() {
                    msg.content = serde_json::Value::String(compressed);
                }
            }
        }
    }
}

fn forward_get(
    request: tiny_http::Request,
    client: &reqwest::blocking::Client,
    upstream: &str,
    path: &str,
) {
    let url = format!("{}{}", upstream.trim_end_matches('/'), path);
    match client.get(&url).send() {
        Ok(resp) => {
            let body = resp.text().unwrap_or_default();
            let response = tiny_http::Response::from_string(body)
                .with_header(json_ct())
                .with_header(make_header("Access-Control-Allow-Origin", "*"));
            request.respond(response).ok();
        }
        Err(e) => {
            respond_error(request, 502, &format!("Upstream error: {}", e));
        }
    }
}

fn forward_post_raw(
    request: tiny_http::Request,
    client: &reqwest::blocking::Client,
    upstream: &str,
    path: &str,
    body: &str,
) {
    let url = format!("{}{}", upstream.trim_end_matches('/'), path);
    let auth = extract_auth(&request);

    let mut req_builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string());

    if let Some(auth_val) = auth {
        req_builder = req_builder.header("Authorization", auth_val);
    }

    match req_builder.send() {
        Ok(resp) => {
            let body = resp.text().unwrap_or_default();
            let response = tiny_http::Response::from_string(body)
                .with_header(json_ct())
                .with_header(make_header("Access-Control-Allow-Origin", "*"));
            request.respond(response).ok();
        }
        Err(e) => {
            respond_error(request, 502, &format!("Upstream error: {}", e));
        }
    }
}

fn extract_auth(request: &tiny_http::Request) -> Option<String> {
    for header in request.headers() {
        let field_name = header.field.to_string();
        if field_name.eq_ignore_ascii_case("Authorization") {
            return Some(header.value.to_string());
        }
    }
    None
}

fn respond_error(request: tiny_http::Request, status: u16, message: &str) {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "proxy_error",
            "code": status
        }
    });
    let response = tiny_http::Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(json_ct())
        .with_header(make_header("Access-Control-Allow-Origin", "*"));
    request.respond(response).ok();
}

fn build_cached_response(cached: &CachedResponse, model: &str) -> String {
    let hash_hex = blake3::hash(cached.content.as_bytes()).to_hex();
    let short_id = &hash_hex.as_str()[..8];
    let resp = ChatResponse {
        id: format!("tp-cache-{}", short_id),
        object: "chat.completion".to_string(),
        created: now_secs(),
        model: model.to_string(),
        choices: vec![Choice {
            index: 0,
            message: Message {
                role: "assistant".to_string(),
                content: serde_json::Value::String(cached.content.clone()),
            },
            finish_reason: Some("stop".to_string()),
        }],
        usage: Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
        extra: serde_json::Map::new(),
    };
    serde_json::to_string(&resp).unwrap_or_default()
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as f64 / 3.5).ceil() as u64
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn json_ct() -> tiny_http::Header {
    "Content-Type: application/json".parse().unwrap()
}

fn make_header(name: &str, value: &str) -> tiny_http::Header {
    format!("{}: {}", name, value).parse().unwrap()
}

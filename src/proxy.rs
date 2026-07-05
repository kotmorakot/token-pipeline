use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Json;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::optimizer::{self, CachedResponse, ResponseCache};
use crate::output_compress;
use crate::stats;

const VERSION: &str = "1.0.0";

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
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

struct ProxyState {
    cache: Mutex<ResponseCache>,
    client: reqwest::Client,
    upstream: String,
    mode: String,
    request_count: AtomicU64,
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cache_hits: AtomicU64,
    total_compressed: AtomicU64,
    is_local_llm: Mutex<Option<bool>>,
}

impl ProxyState {
    fn new(upstream: String, mode: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            cache: Mutex::new(ResponseCache::new()),
            client,
            upstream,
            mode,
            request_count: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_cache_hits: AtomicU64::new(0),
            total_compressed: AtomicU64::new(0),
            is_local_llm: Mutex::new(None),
        }
    }

    fn detect_local(&self) -> bool {
        {
            let cached = self.is_local_llm.lock().unwrap();
            if let Some(val) = *cached {
                return val;
            }
        }

        let is_local = is_private_url(&self.upstream);
        *self.is_local_llm.lock().unwrap() = Some(is_local);
        if is_local {
            eprintln!("  Detected local LLM (private IP) — skipping prompt compression");
        }
        is_local
    }
}

fn is_private_url(url: &str) -> bool {
    let host = url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("");

    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return true;
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                v4.is_private() || v4.is_loopback() || v4.is_link_local()
            }
            IpAddr::V6(v6) => v6.is_loopback(),
        }
    } else {
        false
    }
}

pub fn start_proxy(port: &str, upstream: &str, mode: &str) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(run_server(port, upstream, mode));
}

async fn run_server(port: &str, upstream: &str, mode: &str) {
    let addr = format!("0.0.0.0:{}", port);
    let state = Arc::new(ProxyState::new(upstream.to_string(), mode.to_string()));

    println!();
    println!("  Token Pipeline Proxy v{}", VERSION);
    println!("  {}", "=".repeat(45));
    println!("  Listen:     http://localhost:{}", port);
    println!("  Upstream:   {}", upstream);
    println!("  Compress:   {} mode", mode);
    println!("  Local LLM:  {}", if is_private_url(upstream) { "yes (skip prompt compress)" } else { "no (full optimization)" });
    println!();
    println!("  Endpoints:");
    println!("    POST /v1/chat/completions  optimized (cache + compress)");
    println!("    GET  /v1/models            pass-through");
    println!("    GET  /health               proxy stats");
    println!("    GET  /v1/stats             detailed analytics");
    println!("    POST /v1/cache/clear       clear response cache");
    println!();
    println!("  Configure your tool:");
    println!("    export OPENAI_BASE_URL=http://localhost:{}/v1", port);
    println!();
    println!("  Ctrl+C to stop");
    println!();

    let app = axum::Router::new()
        .route("/v1/chat/completions", post(handle_chat))
        .route("/v1/models", get(handle_models))
        .route("/health", get(handle_health))
        .route("/v1/health", get(handle_health))
        .route("/v1/stats", get(handle_detailed_stats))
        .route("/v1/cache/clear", post(handle_cache_clear))
        .layer(axum::middleware::from_fn(cors_middleware))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    eprintln!("  Proxy listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    eprintln!("\n  Shutting down proxy...");
}

async fn cors_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if request.method() == axum::http::Method::OPTIONS {
        return Response::builder()
            .status(200)
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            .header("Access-Control-Allow-Headers", "Content-Type, Authorization")
            .body(Body::empty())
            .unwrap();
    }

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    response
}

async fn handle_chat(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let req_num = state.request_count.fetch_add(1, Ordering::SeqCst) + 1;

    let mut chat_req: ChatRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return error_response(400, &format!("Invalid JSON: {}", e));
        }
    };

    if chat_req.stream == Some(true) {
        return handle_stream(&state, &headers, &body, req_num).await;
    }

    let messages_json = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let cache_key = ResponseCache::cache_key(&messages_json, &chat_req.model);

    {
        let mut cache = state.cache.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            eprintln!("  [#{}] CACHE HIT (saved ~{} tokens)", req_num, cached.completion_tokens);
            state.total_cache_hits.fetch_add(1, Ordering::SeqCst);
            stats::record_proxy(cached.completion_tokens as u64, 0, true);
            let response_body = build_cached_response(cached, &chat_req.model);
            return json_response(200, &response_body);
        }
    }

    let is_local = state.detect_local();
    let original_msg_count = chat_req.messages.len();
    let original_input = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let original_input_tokens = estimate_tokens(&original_input);

    if !is_local {
        optimize_messages(&mut chat_req.messages, &state.mode);
    }

    let optimized_input = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let optimized_input_tokens = estimate_tokens(&optimized_input);
    let input_savings = original_input_tokens.saturating_sub(optimized_input_tokens);
    if input_savings > 0 {
        eprintln!(
            "  [#{}] prompt: {} -> {} tokens ({} saved)",
            req_num, original_input_tokens, optimized_input_tokens, input_savings
        );
    }

    let optimized_body = match serde_json::to_string(&chat_req) {
        Ok(b) => b,
        Err(e) => return error_response(500, &format!("Serialization error: {}", e)),
    };

    let upstream_url = format!("{}/v1/chat/completions", state.upstream.trim_end_matches('/'));
    let auth = extract_auth(&headers);

    let mut req_builder = state.client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .body(optimized_body);

    if let Some(auth_val) = auth {
        req_builder = req_builder.header("Authorization", auth_val);
    }

    let start_time = SystemTime::now();
    match req_builder.send().await {
        Ok(resp) => {
            let elapsed = start_time.elapsed().unwrap_or_default().as_millis();
            let status = resp.status().as_u16();
            let resp_body = resp.text().await.unwrap_or_default();

            if status != 200 {
                eprintln!("  [#{}] upstream error {} ({}ms)", req_num, status, elapsed);
                return json_response(status, &resp_body);
            }

            let mut chat_resp: ChatResponse = match serde_json::from_str(&resp_body) {
                Ok(r) => r,
                Err(_) => return json_response(200, &resp_body),
            };

            let original_tokens = chat_resp.usage.completion_tokens;

            if !is_local {
                for choice in &mut chat_resp.choices {
                    if let Some(text) = choice.message.content.as_str() {
                        let compressed = output_compress::compress_text(text, &state.mode);
                        choice.message.content = serde_json::Value::String(compressed);
                    }
                }
            }

            let compressed_content = chat_resp
                .choices.first()
                .and_then(|c| c.message.content.as_str())
                .unwrap_or("").to_string();

            let compressed_tokens = estimate_tokens(&compressed_content);
            let savings = original_tokens.saturating_sub(compressed_tokens as u32);

            state.total_input_tokens.fetch_add(original_input_tokens, Ordering::SeqCst);
            state.total_output_tokens.fetch_add(compressed_tokens, Ordering::SeqCst);
            state.total_compressed.fetch_add(savings as u64, Ordering::SeqCst);

            {
                let mut cache = state.cache.lock().unwrap();
                cache.put(cache_key, CachedResponse {
                    content: compressed_content,
                    model: chat_req.model.clone(),
                    prompt_tokens: chat_resp.usage.prompt_tokens,
                    completion_tokens: original_tokens,
                    timestamp: now_secs(),
                });
            }

            stats::record_proxy(original_tokens as u64, compressed_tokens, false);

            eprintln!(
                "  [#{}] {} msgs, {} prompt, {}->~{} output ({} saved) [{}ms]",
                req_num, original_msg_count, chat_resp.usage.prompt_tokens,
                original_tokens, compressed_tokens, savings, elapsed
            );

            let final_body = serde_json::to_string(&chat_resp).unwrap_or(resp_body);
            json_response(200, &final_body)
        }
        Err(e) => {
            eprintln!("  [#{}] upstream failed: {}", req_num, e);
            error_response(502, &format!("Upstream error: {}", e))
        }
    }
}

async fn handle_stream(
    state: &Arc<ProxyState>,
    headers: &HeaderMap,
    body: &str,
    req_num: u64,
) -> Response {
    eprintln!("  [#{}] streaming request", req_num);

    let is_local = state.detect_local();
    let upstream_url = format!("{}/v1/chat/completions", state.upstream.trim_end_matches('/'));
    let auth = extract_auth(headers);

    let mut req_builder = state.client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .body(body.to_string());

    if let Some(auth_val) = auth {
        req_builder = req_builder.header("Authorization", auth_val);
    }

    match req_builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return json_response(status.as_u16(), &body);
            }

            let stream = resp.bytes_stream();

            if is_local {
                let body = Body::from_stream(stream);
                return Response::builder()
                    .status(200)
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("Connection", "keep-alive")
                    .body(body)
                    .unwrap();
            }

            let mode = state.mode.clone();
            let compressed_stream = stream.map(move |chunk| {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        let compressed = compress_sse_chunk(&text, &mode);
                        Ok::<_, reqwest::Error>(axum::body::Bytes::from(compressed))
                    }
                    Err(e) => Err(e),
                }
            });

            let body = Body::from_stream(compressed_stream);
            Response::builder()
                .status(200)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(body)
                .unwrap()
        }
        Err(e) => {
            eprintln!("  [#{}] stream upstream failed: {}", req_num, e);
            error_response(502, &format!("Upstream error: {}", e))
        }
    }
}

fn compress_sse_chunk(chunk: &str, mode: &str) -> String {
    let mut result = String::new();

    for line in chunk.split('\n') {
        if !line.starts_with("data: ") || line.starts_with("data: [DONE]") {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        let json_str = &line[6..];
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(json_str) {
            if let Some(choices) = val.get_mut("choices").and_then(|c| c.as_array_mut()) {
                for choice in choices {
                    if let Some(delta) = choice.get_mut("delta") {
                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            if delta.get("tool_calls").is_none()
                                && delta.get("function_call").is_none()
                            {
                                let compressed = output_compress::compress_text(content, mode);
                                delta["content"] = serde_json::Value::String(compressed);
                            }
                        }
                    }
                }
            }
            result.push_str("data: ");
            result.push_str(&serde_json::to_string(&val).unwrap_or_else(|_| json_str.to_string()));
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

async fn handle_models(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
) -> Response {
    let url = format!("{}/v1/models", state.upstream.trim_end_matches('/'));
    let auth = extract_auth(&headers);

    let mut req = state.client.get(&url);
    if let Some(auth_val) = auth {
        req = req.header("Authorization", auth_val);
    }

    match req.send().await {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            json_response(200, &body)
        }
        Err(e) => error_response(502, &format!("Upstream error: {}", e)),
    }
}

async fn handle_health(State(state): State<Arc<ProxyState>>) -> Json<serde_json::Value> {
    let cache_info = {
        let cache = state.cache.lock().unwrap();
        cache.stats()
    };

    Json(serde_json::json!({
        "status": "ok",
        "version": VERSION,
        "upstream": state.upstream,
        "is_local_llm": state.detect_local(),
        "requests": {
            "total": state.request_count.load(Ordering::SeqCst),
            "cache_hits": state.total_cache_hits.load(Ordering::SeqCst),
        },
        "tokens": {
            "input": state.total_input_tokens.load(Ordering::SeqCst),
            "output": state.total_output_tokens.load(Ordering::SeqCst),
            "compressed": state.total_compressed.load(Ordering::SeqCst),
        },
        "cache": {
            "entries": cache_info.2,
            "hits": cache_info.0,
            "misses": cache_info.1,
        }
    }))
}

async fn handle_detailed_stats(State(state): State<Arc<ProxyState>>) -> Json<serde_json::Value> {
    let cache_info = {
        let cache = state.cache.lock().unwrap();
        cache.stats()
    };

    let req_count = state.request_count.load(Ordering::SeqCst);
    let input = state.total_input_tokens.load(Ordering::SeqCst);
    let output = state.total_output_tokens.load(Ordering::SeqCst);
    let compressed = state.total_compressed.load(Ordering::SeqCst);
    let hits = state.total_cache_hits.load(Ordering::SeqCst);

    let cache_rate = if req_count > 0 {
        hits as f64 / req_count as f64 * 100.0
    } else {
        0.0
    };

    let savings_rate = if input + compressed > 0 {
        compressed as f64 / (input + compressed) as f64 * 100.0
    } else {
        0.0
    };

    Json(serde_json::json!({
        "version": VERSION,
        "upstream": state.upstream,
        "mode": state.mode,
        "is_local_llm": state.detect_local(),
        "requests": req_count,
        "cache_hits": hits,
        "cache_hit_rate_pct": format!("{:.1}", cache_rate),
        "tokens_input": input,
        "tokens_output": output,
        "tokens_saved": compressed,
        "savings_rate_pct": format!("{:.1}", savings_rate),
        "cache_entries": cache_info.2,
        "cost_saved_gpt4o": format!("${:.4}", compressed as f64 / 1_000_000.0 * 2.50),
    }))
}

async fn handle_cache_clear(State(state): State<Arc<ProxyState>>) -> Json<serde_json::Value> {
    {
        let mut cache = state.cache.lock().unwrap();
        *cache = ResponseCache::new();
    }
    optimizer::clear_disk_cache();
    eprintln!("  Cache cleared");
    Json(serde_json::json!({"status": "ok", "message": "Cache cleared"}))
}

fn optimize_messages(messages: &mut Vec<Message>, mode: &str) {
    let has_system = messages.iter().any(|m| m.role == "system");

    if !has_system {
        let system_prompt = output_compress::caveman_system_prompt(mode);
        messages.insert(0, Message {
            role: "system".to_string(),
            content: serde_json::Value::String(system_prompt),
        });
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

fn extract_auth(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn error_response(status: u16, message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "message": message, "type": "proxy_error", "code": status }
    });
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json_response(status: u16, body: &str) -> Response {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
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

/// OpenAI-compatible HTTP proxy that optimizes LLM requests/responses
///
/// Pipeline:
///   Client -> tp proxy -> [compress prompt] -> [check cache] -> upstream LLM
///          <- tp proxy <- [compress response] <- [cache result] <- upstream LLM

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

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
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
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
    println!("  Token Pipeline Proxy v0.1.0");
    println!("  {}", "=".repeat(45));
    println!("  Listen:     http://localhost:{}", port);
    println!("  Upstream:   {}", upstream);
    println!("  Compress:   {} mode", mode);
    println!();
    println!("  Endpoints:");
    println!("    POST /v1/chat/completions  optimized forwarding");
    println!("    GET  /v1/models            pass-through");
    println!("    GET  /health               proxy health check");
    println!();
    println!("  Configure your tool:");
    println!("    export OPENAI_BASE_URL=http://localhost:{}/v1", port);
    println!("    # or set openai.baseUrl in IDE settings");
    println!();
    println!("  Ctrl+C to stop");
    println!();

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("Failed to create HTTP client");

    let mut cache = ResponseCache::new();
    let upstream = upstream.to_string();
    let mode = mode.to_string();
    let mut request_count = 0u64;

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

        request_count += 1;

        match url.as_str() {
            "/v1/chat/completions" => {
                handle_chat(request, &client, &upstream, &mode, &mut cache, request_count);
            }
            "/v1/models" => {
                forward_get(request, &client, &upstream, "/v1/models");
            }
            "/health" => {
                let body = serde_json::json!({
                    "status": "ok",
                    "requests": request_count,
                    "cache_entries": cache.stats().2,
                    "cache_hits": cache.stats().0,
                });
                let response = tiny_http::Response::from_string(body.to_string())
                    .with_header(json_ct());
                request.respond(response).ok();
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
    cache: &mut ResponseCache,
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

    if chat_req.stream == Some(true) {
        eprintln!("  [#{}] streaming request, forwarding raw", req_num);
        forward_post_raw(request, client, upstream, "/v1/chat/completions", &body);
        return;
    }

    let messages_json = serde_json::to_string(&chat_req.messages).unwrap_or_default();
    let cache_key = ResponseCache::cache_key(&messages_json, &chat_req.model);

    if let Some(cached) = cache.get(&cache_key) {
        eprintln!("  [#{}] CACHE HIT saved ~{} tokens", req_num, cached.completion_tokens);
        let response_body = build_cached_response(cached, &chat_req.model);
        stats::record_proxy(cached.completion_tokens as u64, 0, true);
        let response = tiny_http::Response::from_string(response_body)
            .with_header(json_ct())
            .with_header(make_header("Access-Control-Allow-Origin", "*"));
        request.respond(response).ok();
        return;
    }

    let original_msg_count = chat_req.messages.len();
    optimize_messages(&mut chat_req.messages, mode);

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

    match req_builder.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let resp_body = resp.text().unwrap_or_default();

            if status != 200 {
                eprintln!("  [#{}] upstream error {}", req_num, status);
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

            let compressed_tokens = estimate_tokens(
                chat_resp
                    .choices
                    .first()
                    .and_then(|c| c.message.content.as_str())
                    .unwrap_or(""),
            );

            stats::record_proxy(original_tokens as u64, compressed_tokens, false);

            let savings = original_tokens.saturating_sub(compressed_tokens as u32);
            eprintln!(
                "  [#{}] {} msgs, {} prompt, {} -> ~{} completion ({} saved)",
                req_num, original_msg_count, chat_resp.usage.prompt_tokens,
                original_tokens, compressed_tokens, savings
            );

            let final_body = serde_json::to_string(&chat_resp).unwrap_or(resp_body);
            let response = tiny_http::Response::from_string(final_body)
                .with_header(json_ct())
                .with_header(make_header("Access-Control-Allow-Origin", "*"));
            request.respond(response).ok();
        }
        Err(e) => {
            eprintln!("  [#{}] upstream failed: {}", req_num, e);
            respond_error(request, 502, &format!("Upstream error: {}", e));
        }
    }
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

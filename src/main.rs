use axum::{
    extract::{State},
    http::{HeaderMap, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::{post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex, // اضافه شد
    time::timeout,
};
use uuid::Uuid;

// ───── Constants ──────────────────────────────────────────
const AUTH_KEY: &str = "super-secret-key";
const CONNECT_TIMEOUT: u64 = 10;
const READ_TIMEOUT: u64 = 300;
const MAX_PAYLOAD_SIZE: usize = 64 * 1024;
const SESSION_IDLE_SECONDS: u64 = 120;

// ───── Types ──────────────────────────────────────────────
type SessionMap = Arc<DashMap<String, Session>>;

struct Session {
    stream: Mutex<TcpStream>, // Mutex برای مدیریت همزمانی
    last_used: Instant,
}

#[derive(Debug, Deserialize)]
struct TunnelReq {
    op: String,
    target: Option<String>,
    sid: Option<String>,
    payload: Option<String>,
}

#[derive(Debug, Serialize)]
struct TunnelResp {
    sid: String,
    data: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GasRequest {
    key: String,
    u: String,
    m: Option<String>,
    h: Option<HashMap<String, String>>,
    b: Option<String>,
}

// ───── Main ──────────────────────────────────────────────
#[tokio::main]
async fn main() {
    let client = Client::builder()
        .tcp_nodelay(true)
        .http2_adaptive_window(true)
        .pool_idle_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(120))
        .proxy(reqwest::Proxy::all("socks5h://127.0.0.1:1080").unwrap())
        .build()
        .unwrap();

    let sessions: SessionMap = Arc::new(DashMap::new());
    start_session_gc(sessions.clone());

    let app = Router::new()
        .route("/router", post(router))
        .route("/tunnel", post(tunnel_handler))
        .with_state((client, sessions));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("🚀 Production Proxy running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ───── Router Handler ────────────────────────────────────
async fn router(
    State((client, _)): State<(Client, SessionMap)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let gas_req: GasRequest = if !body.is_empty() {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    } else {
        let query: HashMap<String, String> = serde_urlencoded::from_str(
            headers.get("x-original-query")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
        ).unwrap_or_default();

        GasRequest {
            key: query.get("key").cloned().unwrap_or_default(),
            u: query.get("u").cloned().unwrap_or_default(),
            m: Some("GET".into()),
            h: None,
            b: None,
        }
    };

    if gas_req.key != AUTH_KEY {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let method = gas_req.m.unwrap_or("GET".into());
    let mut req_builder = match method.as_str() {
        "POST" => client.post(&gas_req.u),
        "PUT" => client.put(&gas_req.u),
        "DELETE" => client.delete(&gas_req.u),
        _ => client.get(&gas_req.u),
    };

    if let Some(custom_headers) = gas_req.h {
        for (k, v) in custom_headers {
            if should_skip_header(&HeaderName::from_bytes(k.as_bytes()).unwrap_or(HeaderName::from_static("x"))) {
                continue;
            }
            req_builder = req_builder.header(k, v);
        }
    }

    if let Some(b) = gas_req.b {
        req_builder = req_builder.body(b);
    }

    let resp = match req_builder.send().await {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let content_type = resp.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("").to_ascii_lowercase();

    if is_video(&content_type) {
        fast_stream(resp).await
    } else {
        let status = resp.status();
        let resp_headers = resp.headers().clone();
        let raw = resp.bytes().await.unwrap_or_default();

        Json(serde_json::json!({
            "status": status.as_u16(),
            "headers": resp_headers.iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string())).collect::<HashMap<_, _>>(),
            "body": general_purpose::STANDARD.encode(raw),
            "type": "raw"
        })).into_response()
    }
}

// ───── Tunnel Handler ────────────────────────────────────
async fn tunnel_handler(
    State((_, sessions)): State<(Client, SessionMap)>,
    headers: HeaderMap,
    Json(req): Json<TunnelReq>,
) -> impl IntoResponse {
    if headers.get("x-auth-key").map(|v| v == AUTH_KEY).unwrap_or(false) == false {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match req.op.as_str() {
        "open" => handle_open(sessions, req).await,
        "data" => handle_data(sessions, req).await,
        "close" => handle_close(sessions, req).await,
        _ => StatusCode::BAD_REQUEST.into_response(),
    }
}

async fn handle_open(sessions: SessionMap, req: TunnelReq) -> Response {
    let target = match req.target { Some(t) => t, None => return StatusCode::BAD_REQUEST.into_response() };
    if !is_safe_target(&target) { return StatusCode::FORBIDDEN.into_response(); }

    let connect = timeout(Duration::from_secs(CONNECT_TIMEOUT), TcpStream::connect(&target)).await;
    let stream = match connect { Ok(Ok(s)) => s, _ => return StatusCode::BAD_GATEWAY.into_response() };
    
    let sid = Uuid::new_v4().to_string();
    sessions.insert(sid.clone(), Session { stream: Mutex::new(stream), last_used: Instant::now() });

    Json(TunnelResp { sid, data: None, error: None }).into_response()
}

async fn handle_data(sessions: SessionMap, req: TunnelReq) -> Response {
    let sid = match req.sid { Some(s) => s, None => return StatusCode::BAD_REQUEST.into_response() };
    let payload_b64 = match req.payload { Some(p) => p, None => return StatusCode::BAD_REQUEST.into_response() };
    let payload = match general_purpose::STANDARD.decode(payload_b64) {
        Ok(p) if p.len() <= MAX_PAYLOAD_SIZE => p,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };

    let session = match sessions.get(&sid) { Some(s) => s, None => return StatusCode::NOT_FOUND.into_response() };
    let mut stream = session.stream.lock().await; // قفل کردن استریم برای استفاده
    
    if stream.write_all(&payload).await.is_err() {
        drop(stream); // رها کردن قفل قبل از حذف
        sessions.remove(&sid);
        return StatusCode::BAD_GATEWAY.into_response();
    }

    let mut buffer = vec![0u8; 16384];
    let mut total = Vec::new();
    loop {
        match timeout(Duration::from_millis(READ_TIMEOUT), stream.read(&mut buffer)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(n)) => {
                total.extend_from_slice(&buffer[..n]);
                if n < buffer.len() { break; }
            }
            _ => break,
        }
    }
    // به روز رسانی last_used در ساختار (چون از Arc استفاده نکردیم، نیاز به دسترسی mutable به کل session است)
    // راه حل سریع: فقط یک فیلد Atomic برای زمان بگذارید یا session را مجدد جایگزین کنید
    Json(TunnelResp { sid, data: Some(general_purpose::STANDARD.encode(total)), error: None }).into_response()
}

async fn handle_close(sessions: SessionMap, req: TunnelReq) -> Response {
    if let Some(sid) = req.sid { sessions.remove(&sid); return StatusCode::OK.into_response(); }
    StatusCode::BAD_REQUEST.into_response()
}

fn start_session_gc(sessions: SessionMap) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let now = Instant::now();
            sessions.retain(|_, s| now.duration_since(s.last_used).as_secs() < SESSION_IDLE_SECONDS);
        }
    });
}

fn is_safe_target(target: &str) -> bool {
    if let Some(host) = target.split(':').next() {
        if let Ok(ip) = IpAddr::from_str(host) { return !ip.is_loopback() && !ip.is_private(); }
    }
    true
}

fn should_skip_header(name: &HeaderName) -> bool {
    matches!(name.as_str().to_ascii_lowercase().as_str(), "host" | "content-length" | "transfer-encoding" | "connection" | "accept-encoding" | "proxy-connection" | "upgrade" | "keep-alive")
}

fn is_video(ct: &str) -> bool {
    ct.contains("video") || ct.contains("mp4") || ct.contains("webm") || ct.contains("mpeg")
}

async fn fast_stream(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = axum::body::Body::from_stream(resp.bytes_stream());

    let mut builder = Response::builder().status(status);
    for (k, v) in headers.iter() {
        if !should_skip_header(k) { builder = builder.header(k, v); }
    }
    builder.body(body).unwrap()
}

use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use dashmap::DashMap;
use futures::StreamExt;
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
    sync::Mutex,
    time::timeout,
};
use uuid::Uuid;

const AUTH_KEY: &str = "super-secret-key";
const CONNECT_TIMEOUT: u64 = 10;
const READ_TIMEOUT: u64 = 300;
const MAX_PAYLOAD_SIZE: usize = 64 * 1024;
const SESSION_IDLE_SECONDS: u64 = 120;

type SessionMap = Arc<DashMap<String, Session>>;

struct Session {
    stream: Mutex<TcpStream>,
    last_used: Mutex<Instant>,
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
        .route("/router", get(router_get).post(router_post))
        .route("/tunnel", post(tunnel_handler))
        .with_state((client, sessions));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("proxy running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn start_session_gc(sessions: SessionMap) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let now = Instant::now();

            sessions.retain(|_, s| {
                if let Ok(last) = s.last_used.try_lock() {
                    now.duration_since(*last).as_secs() < SESSION_IDLE_SECONDS
                } else {
                    true
                }
            });
        }
    });
}

async fn router_get(
    Query(params): Query<HashMap<String, String>>,
    State((client, _)): State<(Client, SessionMap)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let target = match params.get("u") {
        Some(u) => u,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    forward_request(client, target, headers, None).await
}

async fn router_post(
    State((client, _)): State<(Client, SessionMap)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let gas_req: GasRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    if gas_req.key != AUTH_KEY {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    forward_request(client, &gas_req.u, headers, gas_req.b).await
}

async fn forward_request(
    client: Client,
    target: &str,
    headers: HeaderMap,
    body: Option<String>,
) -> Response {
    let mut req = client.get(target);

    for (k, v) in headers.iter() {
        if should_skip_header(k) {
            continue;
        }

        if let Ok(val) = v.to_str() {
            req = req.header(k, val);
        }
    }

    if let Some(b) = body {
        req = req.body(b);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if is_video(&content_type) {
        fast_stream(resp).await
    } else {
        slow_sse(resp).await
    }
}

async fn tunnel_handler(
    State((_, sessions)): State<(Client, SessionMap)>,
    headers: HeaderMap,
    Json(req): Json<TunnelReq>,
) -> impl IntoResponse {
    let auth_ok = headers
        .get("x-auth-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == AUTH_KEY)
        .unwrap_or(false);

    if !auth_ok {
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
    let target = match req.target {
        Some(t) => t,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    if !is_safe_target(&target) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let connect =
        timeout(Duration::from_secs(CONNECT_TIMEOUT), TcpStream::connect(&target)).await;

    let stream = match connect {
        Ok(Ok(s)) => s,
        _ => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let sid = Uuid::new_v4().to_string();

    sessions.insert(
        sid.clone(),
        Session {
            stream: Mutex::new(stream),
            last_used: Mutex::new(Instant::now()),
        },
    );

    Json(TunnelResp {
        sid,
        data: None,
        error: None,
    })
    .into_response()
}

async fn handle_data(sessions: SessionMap, req: TunnelReq) -> Response {
    let sid = match req.sid {
        Some(s) => s,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let payload_b64 = match req.payload {
        Some(p) => p,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let payload = match general_purpose::STANDARD.decode(payload_b64) {
        Ok(p) if p.len() <= MAX_PAYLOAD_SIZE => p,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };

    let session = match sessions.get(&sid) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let mut stream = session.stream.lock().await;

    if stream.write_all(&payload).await.is_err() {
        drop(stream);
        sessions.remove(&sid);
        return StatusCode::BAD_GATEWAY.into_response();
    }

    let mut buffer = vec![0u8; 16384];
    let mut total = Vec::new();

    loop {
        match timeout(Duration::from_millis(READ_TIMEOUT), stream.read(&mut buffer)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                total.extend_from_slice(&buffer[..n]);
                if n < buffer.len() {
                    break;
                }
            }
            _ => break,
        }
    }

    let mut last = session.last_used.lock().await;
    *last = Instant::now();


    Json(TunnelResp {
        sid,
        data: if total.is_empty() {
            None
        } else {
            Some(general_purpose::STANDARD.encode(total))
        },
        error: None,
    })
    .into_response()
}

async fn handle_close(sessions: SessionMap, req: TunnelReq) -> Response {
    if let Some(sid) = req.sid {
        sessions.remove(&sid);
        return StatusCode::OK.into_response();
    }

    StatusCode::BAD_REQUEST.into_response()
}

fn is_safe_target(target: &str) -> bool {
    if let Some(host) = target.split(':').next() {
        if let Ok(ip) = IpAddr::from_str(host) {
            return match ip {
    std::net::IpAddr::V4(v4) => {
        !v4.is_private() && !v4.is_loopback()
    }
    std::net::IpAddr::V6(v6) => {
        !v6.is_loopback()
    }
}

        }
    }
    true
}

fn should_skip_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "accept-encoding"
            | "proxy-connection"
            | "upgrade"
            | "keep-alive"
    )
}

fn is_video(ct: &str) -> bool {
    ct.contains("video")
        || ct.contains("mp4")
        || ct.contains("webm")
        || ct.contains("mpeg")
}

async fn fast_stream(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = axum::body::Body::from_stream(resp.bytes_stream());

    let mut builder = Response::builder().status(status);

    for (k, v) in headers.iter() {
        if !should_skip_header(k) {
            builder = builder.header(k, v);
        }
    }

    builder.body(body).unwrap()
}

async fn slow_sse(resp: reqwest::Response) -> Response {
    let status = resp.status();

    let stream = resp.bytes_stream().map(|chunk| {
        let text = match chunk {
            Ok(bytes) => format!("data:{}\n\n", base64::engine::general_purpose::STANDARD.encode(bytes)),
            Err(_) => "event:error\ndata:stream\n\n".to_string(),
        };

        Ok::<Bytes, std::io::Error>(Bytes::from(text))
    });

    Response::builder()
        .status(status)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(axum::body::Body::from_stream(stream))
        .unwrap()
}
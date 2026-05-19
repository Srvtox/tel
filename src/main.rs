use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;
use std::{collections::HashMap, net::SocketAddr, time::Duration};

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

    let app = Router::new()
        .route("/router", get(router))
        .with_state(client);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    println!("🚀 Proxy running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn router(
    Query(params): Query<HashMap<String, String>>,
    State(client): State<Client>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let target = match params.get("u") {
        Some(u) => u,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let mut req = client.get(target);

    for (k, v) in headers.iter() {
        if should_skip_header(k) {
            continue;
        }

        if let Ok(val) = v.to_str() {
            req = req.header(k, val);
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(err) => {
            eprintln!("❌ Upstream error: {}", err);
            return StatusCode::BAD_GATEWAY.into_response();
        }
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

fn should_skip_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "host" | "content-length" | "transfer-encoding" | "connection" | "accept-encoding"
    )
}

fn is_video(ct: &str) -> bool {
    ct.contains("video")
        || ct.contains("mp4")
        || ct.contains("webm")
        || ct.contains("mpeg")
        || ct.contains("ogg")
}

async fn fast_stream(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let headers = resp.headers().clone();
    let stream = resp.bytes_stream();

    let body = axum::body::Body::from_stream(stream);

    let mut builder = Response::builder().status(status);

    for (k, v) in headers.iter() {
        if should_skip_header(k) {
            continue;
        }
        builder = builder.header(k, v);
    }

    builder.body(body).unwrap()
}

async fn slow_sse(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let stream = resp.bytes_stream();

    let mapped = stream.map(|chunk| {
        let text = match chunk {
            Ok(bytes) => {
                let encoded = general_purpose::STANDARD.encode(bytes);
                format!("data:{}\n\n", encoded)
            }
            Err(_) => "event:error\ndata:stream\n\n".to_string(),
        };

        Ok::<Bytes, std::io::Error>(Bytes::from(text))
    });

    Response::builder()
        .status(status)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(axum::body::Body::from_stream(mapped))
        .unwrap()
}

//! Dev/test harness: serves web/dist and exposes `Core::api` over HTTP at /api/* (same contract
//! the desktop IPC uses), so Playwright can test the real UI against the Rust core. Not shipped.
//! Usage: VAULTLM_DATA=/tmp/data VAULT=/tmp/vault cargo run -p vaultlm-core --example devserver -- 3797
use axum::{body::Bytes, extract::{Query, State}, http::{Method, StatusCode, Uri}, response::{IntoResponse, Response}, routing::any, Json, Router};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use vaultlm_core::Core;

async fn api(State(core): State<Arc<Core>>, method: Method, uri: Uri, Query(q): Query<HashMap<String, String>>, body: Bytes) -> Response {
    let body: Value = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap_or(Value::Null) };
    match core.api(method.as_str(), uri.path(), json!(q), body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            let mut v = json!({ "message": e.message });
            if let Some(Value::Object(x)) = e.extra { v.as_object_mut().unwrap().extend(x); }
            (StatusCode::from_u16(e.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(v)).into_response()
        }
    }
}

// POST /api/chat → newline-delimited JSON, like the original server.
async fn chat(State(core): State<Arc<Core>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<String, std::io::Error>>();
    tokio::spawn(async move {
        let send = { let tx = tx.clone(); move |v: Value| { let _ = tx.send(Ok(format!("{v}\n"))); } };
        if let Err(e) = core.stream("/api/chat", body, &send).await { let _ = tx.send(Ok(format!("{}\n", json!({ "error": e.message })))); }
        let _ = tx.send(Ok(format!("{}\n", json!({ "done": true }))));
    });
    let stream = futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|x| (x, rx)) });
    axum::body::Body::from_stream(stream).into_response()
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(3797);
    let core = Core::new(vaultlm_core::paths::data_dir(), Arc::new(|name: &str, _p: Value| eprintln!("event: {name}")));
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    let app = Router::new()
        .route("/api/chat", axum::routing::post(chat))
        .route("/api/{*rest}", any(api))
        .fallback_service(tower_http::services::ServeDir::new(dist))
        .with_state(core);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    eprintln!("devserver on http://127.0.0.1:{port}");
    axum::serve(listener, app).await.unwrap();
}

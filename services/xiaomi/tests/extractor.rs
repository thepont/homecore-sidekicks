#![cfg(feature = "extractor")]

use axum::{
    body::Body,
    extract::State,
    http::{header, Response},
    routing::get,
    Json, Router,
};
use homecore_xiaomi::{extractor, login::Provider};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Notify};

#[tokio::test]
async fn the_worker_streams_a_qr_before_the_xiaomi_sign_in_finishes() {
    let source = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_url = format!("http://{}", source.local_addr().unwrap());
    let approved = Arc::new(Notify::new());
    let app = Router::new()
        .route(
            "/longPolling/loginUrl",
            get(|State((url, _)): State<(String, Arc<Notify>)>| async move {
                Json(json!({"qr":format!("{url}/qr"),"lp":format!("{url}/poll"),"timeout":10}))
            }),
        )
        .route(
            "/qr",
            get(|| async {
                Response::builder()
                    .header(header::CONTENT_TYPE, "image/png")
                    .body(Body::from(b"\x89PNG\r\n\x1a\n".as_slice()))
                    .unwrap()
            }),
        )
        .route(
            "/poll",
            get(
                |State((_, approval)): State<(String, Arc<Notify>)>| async move {
                    approval.notified().await;
                    Json(json!({"error":"cancelled"}))
                },
            ),
        )
        .with_state((source_url.clone(), approved.clone()));
    let upstream = tokio::spawn(async move { axum::serve(source, app).await.unwrap() });
    let listen = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/discover", listen.local_addr().unwrap());
    let worker = tokio::spawn(async move {
        axum::serve(
            listen,
            extractor::router(Provider::new(&source_url).unwrap(), [0xab; 32]),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let request = json!({"region":"cn","models":["ijai.vacuum.v10"]});
    assert_eq!(
        client
            .post(&url)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let mut response = client
        .post(&url)
        .bearer_auth("ab".repeat(32))
        .json(&request)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let first = response.chunk().await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&first).unwrap(),
        json!({"type":"qr","qr_png":"iVBORw0KGgo="})
    );
    assert_eq!(
        client
            .post(&url)
            .bearer_auth("ab".repeat(32))
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    approved.notify_one();
    assert_eq!(
        serde_json::from_str::<Value>(&response.text().await.unwrap()).unwrap()["type"],
        "error"
    );
    worker.abort();
    upstream.abort();
}

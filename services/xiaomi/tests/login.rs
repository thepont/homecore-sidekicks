use axum::{
    body::Body,
    extract::State,
    http::{header, Response},
    routing::get,
    Json, Router,
};
use homecore_xiaomi::login::Provider;
use serde_json::json;

#[tokio::test]
async fn creates_a_qr_challenge_from_the_provider() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let app = Router::new()
        .route(
            "/longPolling/loginUrl",
            get(|State(base): State<String>| async move {
                Json(json!({"qr":format!("{base}/qr"),"lp":format!("{base}/poll"),"timeout":120}))
            }),
        )
        .route(
            "/qr",
            get(|| async {
                Response::builder()
                    .header(header::CONTENT_TYPE, "image/png")
                    .body(Body::from(vec![137, 80, 78, 71, 13, 10, 26, 10]))
                    .unwrap()
            }),
        )
        .with_state(base.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let result = Provider::new(&base).unwrap().begin().await.unwrap();
    assert_eq!(result.qr_png, "iVBORw0KGgo=");
    server.abort();
}

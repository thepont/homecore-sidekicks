use axum::{body::Bytes, routing::get, Router};
use homecore_teslemetry::totals::{listen, TariffState};
use std::time::Duration;

#[tokio::test]
async fn totals_stream_accepts_multiline_events_split_across_chunks() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = format!(
        "event: energy_totals\r\ndata: {{\"id\":\"42\",\r\ndata: \"date\":\"{}\",\"createdAt\":\"{}\",\r\ndata: \"totals\":{{\"total_solar_generation\":12500}}}}\r\n\r\n",
        now.format("%Y-%m-%d"),
        now.to_rfc3339()
    );
    let app = Router::new().route(
        "/sse",
        get(move || {
            let event = event.clone();
            async move {
                let pieces = event
                    .as_bytes()
                    .chunks(7)
                    .map(|piece| Ok::<_, std::io::Error>(Bytes::copy_from_slice(piece)))
                    .collect::<Vec<_>>();
                (
                    [("content-type", "text/event-stream")],
                    axum::body::Body::from_stream(futures_util::stream::iter(pieces)),
                )
            }
        }),
    );
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", socket.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(socket, app).await.unwrap() });
    let (sender, mut readings) = tokio::sync::watch::channel(Default::default());
    let (tariffs, _) = tokio::sync::watch::channel(TariffState::default());
    let listener = tokio::spawn(listen(
        base,
        42,
        "fixture-token".to_owned(),
        sender,
        tariffs,
    ));
    let reading = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            readings.changed().await.unwrap();
            if let Some(value) = readings.borrow().get("total_solar_generation_wh") {
                break value.clone();
            }
        }
    })
    .await;
    listener.abort();
    server.abort();
    assert_eq!(reading.unwrap(), serde_json::json!(12500.0));
}

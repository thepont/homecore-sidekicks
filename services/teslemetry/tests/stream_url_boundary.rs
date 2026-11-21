use std::time::Duration;

const INVALID_BASE: &str = "http://127.0.0.1:9/?query=value";

#[tokio::test]
async fn totals_listener_rejects_a_base_url_with_a_query() {
    let (readings, _) = tokio::sync::watch::channel(Default::default());
    let (tariffs, _) = tokio::sync::watch::channel(Default::default());
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        homecore_teslemetry::totals::listen(
            INVALID_BASE.to_owned(),
            42,
            "fixture-token".to_owned(),
            readings,
            tariffs,
        ),
    )
    .await;
    assert!(result.is_ok(), "invalid URL must stop the totals listener");
}

#[tokio::test]
async fn vehicle_supervisor_rejects_a_base_url_with_a_query() {
    let broker = homecore_teslemetry::vehicle::Broker {
        host: "127.0.0.1".to_owned(),
        port: 1,
        username: String::new(),
        password: String::new(),
    };
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        homecore_teslemetry::vehicle::supervise(
            INVALID_BASE.to_owned(),
            "fixture-token".to_owned(),
            broker,
        ),
    )
    .await;
    assert!(
        result.is_ok(),
        "invalid URL must stop the vehicle supervisor"
    );
}

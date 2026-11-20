#[path = "support/homecore.rs"]
mod homecore;
mod support;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::{
    fs,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

struct Adapter(Child);
impl Drop for Adapter {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Source(tokio::task::JoinHandle<()>);
impl Drop for Source {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
#[ignore = "requires Docker and the pinned homeCore image; run explicitly"]
async fn homecore_fetches_the_owned_card_through_its_authenticated_ui_endpoint() {
    tokio::time::timeout(Duration::from_secs(70), verify())
        .await
        .expect("bounded core contract fixture");
}

async fn verify() {
    let core = homecore::Core::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let polled = Arc::new(tokio::sync::Notify::new());
    let first_poll = polled.clone();
    let fake = Router::new().route("/api/1/energy_sites/42/live_status", get(move || {
        requests.fetch_add(1, Ordering::SeqCst);
                first_poll.notify_one();
        async { Json(json!({"response": {
            "timestamp": chrono::DateTime::<chrono::Utc>::from(SystemTime::now()).to_rfc3339(),
            "percentage_charged": 64.2, "battery_power": -1120,
            "solar_power": 2520, "load_power": 680, "grid_power": -720
        }})) }
    })).route("/api/1/vehicles", get(|| async { Json(json!({"response": []})) }));
    let (source_url, task) = support::serve(fake).await;
    let _source = Source(task);
    fs::write(core.root.join("source-token"), "fixture-source-token").unwrap();
    let ui_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let _adapter = Adapter(
        Command::new(env!("CARGO_BIN_EXE_teslemetry-bridge"))
            .env("TESLEMETRY_URL", source_url)
            .env("TESLEMETRY_SITE_ID", "42")
            .env("TESLEMETRY_TOKEN_FILE", core.root.join("source-token"))
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", core.mqtt_port.to_string())
            .env("MQTT_USERNAME", "plugin.teslemetry-powerwall")
            .env("MQTT_PASSWORD_FILE", core.root.join("mqtt-password"))
            .env("POLL_INTERVAL_SECS", "3600")
            .env("HC_PLUGIN_UI_DIR", &ui_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let ui = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let list: Value = core
                .get("/plugins/ui")
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if let Some(ui) = list.as_array().and_then(|v| {
                v.iter()
                    .find(|x| x["plugin_id"] == "plugin.teslemetry-powerwall")
            }) {
                break ui["ui"].clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Teslemetry card announced to core");
    assert_eq!(ui["host_api"], 1);
    assert!(ui["slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["slot"] == "device.card"));
    for element in [
        "hc-teslemetry-solar",
        "hc-teslemetry-grid",
        "hc-teslemetry-battery",
    ] {
        assert!(ui["slots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["slot"] == "dashboard.card" && s["element"] == element));
    }
    let entry = ui["entry"].as_str().unwrap();
    let build = ui["build"].as_str().unwrap();
    let path = format!("/plugins/plugin.teslemetry-powerwall/ui/{build}/{entry}");
    assert_eq!(
        core.anonymous_get(&path).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    // Wait for the independent source poll before checking that requesting the card adds none.
    tokio::time::timeout(Duration::from_secs(3), polled.notified())
        .await
        .expect("first telemetry poll");
    let before = count.load(Ordering::SeqCst);
    let response = core.get(&path).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.headers()["cache-control"],
        "private, max-age=31536000, immutable"
    );
    assert!(response.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/javascript"));
    let bytes = response.bytes().await.unwrap();
    assert_eq!(bytes.as_ref(), fs::read(ui_dir.join(entry)).unwrap());
    assert_eq!(before, count.load(Ordering::SeqCst));
    let stale = core
        .get(&format!(
            "/plugins/plugin.teslemetry-powerwall/ui/old-build/{entry}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), reqwest::StatusCode::GONE);
    let missing = core
        .get(&format!(
            "/plugins/plugin.teslemetry-powerwall/ui/{build}/missing.js"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
    let device: Value = core
        .get("/devices/teslemetry_powerwall")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(device["plugin_id"], "plugin.teslemetry-powerwall");
}

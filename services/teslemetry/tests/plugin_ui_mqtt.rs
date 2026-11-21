use homecore_test_support::mqtt_broker;

use rumqttc::QoS;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

const PREFIX: &str = "homecore/plugins/plugin.teslemetry-powerwall";
const REQUEST: &str = "12345678-1234-4234-8234-123456789abc";

struct Adapter {
    child: Child,
    root: PathBuf,
}

impl Drop for Adapter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for file in ["token", "mqtt-password", "manifest.json", "index.js"] {
            let _ = std::fs::remove_file(self.root.join(file));
        }
        let _ = std::fs::remove_dir(&self.root);
    }
}

fn start(port: u16, name: &str, with_ui: bool) -> Adapter {
    let root = std::env::temp_dir().join(format!("teslemetry-ui-{name}-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("token"), "fixture-token").unwrap();
    std::fs::write(root.join("mqtt-password"), "fixture-password").unwrap();
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&json!({
            "host_api": 1, "entry": "index.js", "slots": [{
                "slot": "device.card", "element": "hc-teslemetry-card",
                "device_types": ["sensor"], "forms": ["compact", "standard", "large"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(root.join("index.js"), b"export {};\n".repeat(20_000)).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_teslemetry-bridge"))
        .env("TESLEMETRY_URL", "http://127.0.0.1:9")
        .env("TESLEMETRY_SITE_ID", "42")
        .env("TESLEMETRY_TOKEN_FILE", root.join("token"))
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", root.join("mqtt-password"))
        .env("POLL_INTERVAL_SECS", "3600")
        .env(
            "HC_PLUGIN_UI_DIR",
            if with_ui {
                root.clone()
            } else {
                root.join("missing")
            },
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Adapter { child, root }
}

async fn message(probe: &mut mqtt_broker::Probe, suffix: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(p) = probe.messages.recv().await {
            if p.topic == format!("{PREFIX}/{suffix}") {
                return serde_json::from_slice(&p.payload).unwrap();
            }
        }
        panic!("probe closed");
    })
    .await
    .expect("plugin message must arrive")
}

async fn request(probe: &mut mqtt_broker::Probe, body: Value) -> Value {
    probe
        .client
        .publish(
            format!("{PREFIX}/manage/cmd"),
            QoS::AtLeastOnce,
            false,
            serde_json::to_vec(&body).unwrap(),
        )
        .await
        .unwrap();
    message(probe, "manage/response").await
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn card_assets_are_served_by_the_telemetry_identity_without_a_live_source() {
    use base64::Engine;
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let adapter = start(broker.port, "delivery", true);
    let caps = message(&mut probe, "capabilities").await;
    assert_eq!(caps["plugin_id"], "plugin.teslemetry-powerwall");
    assert_eq!(caps["spec"], "1");
    assert_eq!(caps["actions"], json!([]));
    assert_eq!(caps["ui"]["slots"][0]["slot"], "device.card");
    assert_eq!(caps["ui"]["entry"], "index.js");
    assert_eq!(caps["ui"]["build"].as_str().unwrap().len(), 16);
    // This probe subscribed before adapter startup, so the first non-retained
    // heartbeat cannot have been missed.
    let heartbeat = message(&mut probe, "heartbeat").await;
    assert_eq!(heartbeat["version"], env!("CARGO_PKG_VERSION"));

    let mut bytes = Vec::new();
    loop {
        let response = request(
            &mut probe,
            json!({"action":"ui_file", "request_id":REQUEST,
            "file":"index.js", "offset":bytes.len(), "max":131072}),
        )
        .await;
        assert_eq!(response["request_id"], REQUEST);
        assert_eq!(response["status"], "ok");
        let chunk = base64::engine::general_purpose::STANDARD
            .decode(response["data"].as_str().unwrap())
            .unwrap();
        assert!(chunk.len() <= 131072);
        bytes.extend(chunk);
        if response["done"] == true {
            break;
        }
    }
    assert_eq!(bytes, std::fs::read(adapter.root.join("index.js")).unwrap());
    for body in [
        json!({"action":"ui_file", "request_id":REQUEST,"file":"../token"}),
        json!({"action":"ui_file", "request_id":REQUEST,"file":"token"}),
        json!({"action":"ui_file", "request_id":REQUEST,"file":"index.js","offset":-1}),
        json!({"action":"set_config", "request_id":REQUEST}),
    ] {
        let response = request(&mut probe, body).await;
        assert_eq!(response["status"], "error");
        assert!(!response.to_string().contains("fixture-token"));
    }
    let response = request(&mut probe, json!({"action":"ping", "request_id":REQUEST})).await;
    assert_eq!(response["status"], "ok");
    // A fresh subscriber sees the retained announcement, not a retained response.
    let mut late = mqtt_broker::Probe::start(broker.port).await;
    assert_eq!(message(&mut late, "capabilities").await, caps);
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn card_announcements_and_requests_recover_after_broker_restart() {
    let mut broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let _adapter = start(broker.port, "reconnect", true);
    let before = message(&mut probe, "capabilities").await;
    broker.restart().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let after = message(&mut probe, "capabilities").await;
    assert_eq!(after, before);
    let response = request(
        &mut probe,
        json!({"action":"ui_file", "request_id":REQUEST,
        "file":"index.js", "offset":0,"max":9}),
    )
    .await;
    assert_eq!(response["status"], "ok");
    assert_eq!(response["done"], false);
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn missing_ui_clears_the_announcement_without_stopping_telemetry() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let _adapter = start(broker.port, "missing", false);
    let caps = message(&mut probe, "capabilities").await;
    assert!(caps["ui"].is_null());
    let response = request(&mut probe, json!({"action":"ping", "request_id":REQUEST})).await;
    assert_eq!(response["status"], "ok");
    let response = request(
        &mut probe,
        json!({"action":"ui_file", "request_id":REQUEST,
        "file":"index.js"}),
    )
    .await;
    assert_eq!(response["status"], "error");
    // A fresh subscriber sees the telemetry registration and unavailable reading
    // even though UI discovery failed and the fixture has no live Tesla source.
    let mut late = mqtt_broker::Probe::start(broker.port).await;
    let registration = message(&mut late, "register").await;
    assert_eq!(registration["device_id"], "teslemetry_powerwall");
}

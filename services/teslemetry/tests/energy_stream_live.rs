#[allow(dead_code)]
#[path = "support/mqtt_broker.rs"]
mod mqtt_broker;
use std::{
    env, fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct OwnedChild(Child, PathBuf);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = fs::remove_file(self.1.join("mqtt-password"));
        let _ = fs::remove_dir(&self.1);
    }
}

#[tokio::test]
#[ignore = "requires private Teslemetry credential, Docker and network; read-only source"]
async fn real_stream_and_live_site_are_published_to_fake_broker() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let token = env::var("TESLEMETRY_TOKEN_FILE").unwrap();
    let site = fs::read_to_string(env::var("TESLEMETRY_SITE_ID_FILE").unwrap()).unwrap();
    let root = std::env::temp_dir().join(format!("energy-live-fixture-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("mqtt-password"), "fixture-password").unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_teslemetry-bridge"))
        .env("TESLEMETRY_URL", "https://api.teslemetry.com")
        .env("TESLEMETRY_SITE_ID", site.trim())
        .env("TESLEMETRY_TOKEN_FILE", token)
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", root.join("mqtt-password"))
        .env("POLL_INTERVAL_SECS", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _owned = OwnedChild(child, root);
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                let state: serde_json::Value = serde_json::from_slice(&message.payload).unwrap();
                if state["total_solar_generation_wh"].is_number()
                    && state["site_backup_reserve_pct"].is_number()
                {
                    return true;
                }
            }
        }
        false
    })
    .await;
    assert_eq!(result, Ok(true), "real stream readings should reach MQTT");
}

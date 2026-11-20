#[path = "../../teslemetry/tests/support/mqtt_broker.rs"]
mod mqtt_broker;
use homecore_xiaomi::{login::Provider, setup::Setup, store::Store};
use rumqttc::QoS;
use serde_json::{json, Value};
use std::{fs, time::Duration};

#[tokio::test]
async fn unconfigured_plugin_exposes_admin_setup_actions() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let broker = mqtt_broker::Broker::start().await;
            let mut probe = mqtt_broker::Probe::start(broker.port).await;
            let root = std::env::temp_dir().join(format!("xiaomi-plugin-{}", std::process::id()));
            fs::create_dir(&root).unwrap();
            let setup = Setup::new(
                Provider::new("https://account.xiaomi.com/").unwrap(),
                Store::new(&root),
                "cn",
            );
            let task = tokio::task::spawn_local(homecore_xiaomi::runtime::run(
                "127.0.0.1".into(),
                broker.port,
                "fixture-user".into(),
                "fixture-password".into(),
                setup,
                "ui".into(),
            ));
            let caps = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let p = probe.messages.recv().await.unwrap();
                    if p.topic == "homecore/plugins/fixture-user/capabilities" {
                        break serde_json::from_slice::<Value>(&p.payload).unwrap();
                    }
                }
            })
            .await
            .unwrap();
            assert!(
                caps.get("ui").is_some(),
                "the setup page must be offered to homeCore"
            );
            assert!(caps["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "begin_sign_in" && a["requires_role"] == "admin"));
            probe
                .client
                .publish(
                    "homecore/plugins/fixture-user/manage/cmd",
                    QoS::AtLeastOnce,
                    false,
                    json!({"action":"sign_in_status", "request_id":"fixture"}).to_string(),
                )
                .await
                .unwrap();
            let reply = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let p = probe.messages.recv().await.unwrap();
                    if p.topic.contains("manage/")
                        && p.topic != "homecore/plugins/fixture-user/manage/cmd"
                    {
                        if let Ok(v) = serde_json::from_slice::<Value>(&p.payload) {
                            if v["request_id"] == "fixture" {
                                break v;
                            }
                        }
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(reply["status"], "ok");
            assert_eq!(reply["setup_status"], "unconfigured");
            task.abort();
            fs::remove_dir_all(root).unwrap();
        })
        .await;
}

#[tokio::test]
async fn service_process_registers_without_xiaomi_credentials() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let root = std::env::temp_dir().join(format!("xiaomi-process-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("mqtt-password"), "fixture-password").unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_homecore-xiaomi"))
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", root.join("mqtt-password"))
        .env("XIAOMI_STATE_DIR", &root)
        .env("XIAOMI_UI_DIR", "ui")
        .spawn()
        .unwrap();
    let found = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let p = probe.messages.recv().await.unwrap();
            if p.topic == "homecore/plugins/fixture-user/capabilities" {
                break true;
            }
        }
    })
    .await;
    let _ = child.kill();
    let _ = child.wait();
    fs::remove_dir_all(root).unwrap();
    assert_eq!(found.unwrap(), true);
}

use homecore_test_support::mqtt_broker;
use serde_json::{json, Value};
use std::time::Duration;

struct Process(std::process::Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn process_exposes_setup_before_any_accessory_is_paired() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let root = std::env::temp_dir().join(format!("homekit-setup-process-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let password = root.join("broker-password");
    fs::write(&password, "fixture-password").unwrap();
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let process = Process(
        Command::new(env!("CARGO_BIN_EXE_homecore-homekit"))
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", broker.port.to_string())
            .env("MQTT_USERNAME", "fixture-user")
            .env("MQTT_PASSWORD_FILE", password)
            .env("HOMEKIT_PLUGIN_ID", "fixture-user")
            .env("HOMEKIT_STATE_DIR", &root)
            .env("HOMEKIT_UI_DIR", ui)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
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
    assert!(caps["ui"]["slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["element"] == "hc-homekit-setup"));
    drop(process);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn unconfigured_service_advertises_admin_setup_and_rejects_missing_pairing_details() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let broker = mqtt_broker::Broker::start().await;
            let mut probe = mqtt_broker::Probe::start(broker.port).await;
            let root = std::env::temp_dir().join(format!("homekit-setup-{}", std::process::id()));
            std::fs::create_dir(&root).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            let task = tokio::task::spawn_local(homecore_homekit::setup::run(
                "127.0.0.1".into(),
                broker.port,
                "fixture-user".into(),
                "fixture-password".into(),
                root.clone(),
                std::path::PathBuf::from("ui"),
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
            assert!(caps.get("ui").is_some());
            for action in ["discover_accessories", "pair_accessory"] {
                assert!(caps["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["id"] == action && a["requires_role"] == "admin"));
            }
            probe
                .client
                .publish(
                    "homecore/plugins/fixture-user/manage/cmd",
                    rumqttc::QoS::AtLeastOnce,
                    false,
                    json!({"action":"pair_accessory","request_id":"bad"}).to_string(),
                )
                .await
                .unwrap();
            let reply = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let p = probe.messages.recv().await.unwrap();
                    if p.topic.contains("manage/") && !p.topic.ends_with("/cmd") {
                        if let Ok(v) = serde_json::from_slice::<Value>(&p.payload) {
                            if v["request_id"] == "bad" {
                                break v;
                            }
                        }
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(reply["status"], "error");
            task.abort();
            std::fs::remove_dir_all(root).unwrap();
        })
        .await;
}

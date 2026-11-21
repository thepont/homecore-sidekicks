#[path = "../../teslemetry/tests/support/mqtt_broker.rs"]
mod mqtt_broker;
use homecore_xiaomi::{login::Provider, setup::Setup, store::Store};
use rumqttc::QoS;
use serde_json::{json, Value};
use std::{fs, time::Duration};

#[tokio::test]
async fn installed_adapter_list_survives_a_service_restart() {
    assert_persistent_adapters("sqlite", &[]).await;
}

#[tokio::test]
#[ignore = "requires a disposable TLS PostgreSQL database and a build with the postgres feature"]
async fn installed_adapter_list_survives_a_postgres_service_restart() {
    let database = std::env::var_os("XIAOMI_TEST_POSTGRES_URL_FILE").unwrap();
    let ca = std::env::var_os("XIAOMI_TEST_POSTGRES_CA_FILE").unwrap();
    assert_persistent_adapters(
        "postgres",
        &[
            ("XIAOMI_POSTGRES_URL_FILE", database),
            ("XIAOMI_POSTGRES_CA_FILE", ca),
        ],
    )
    .await;
}

async fn assert_persistent_adapters(backend: &str, extra_env: &[(&str, std::ffi::OsString)]) {
    struct Process(std::process::Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    struct StateDirectory(std::path::PathBuf);
    impl Drop for StateDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    let root =
        std::env::temp_dir().join(format!("xiaomi-adapters-{backend}-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _state_directory = StateDirectory(root.clone());
    let passwords = root.join("mqtt-password");
    fs::write(&passwords, "fixture-password").unwrap();
    let registrations = json!([
        {"plugin_id":"plugin.xiaomi-gateway","models":["lumi.gateway.mcn001"]},
        {"plugin_id":"plugin.xiaomi-vacuum","models":["ijai.vacuum.v10"]}
    ]);
    let source = root.join("adapters.json");
    fs::write(&source, registrations.to_string()).unwrap();
    let mut results = Vec::new();

    for attempt in 0..2 {
        let broker = mqtt_broker::Broker::start().await;
        let mut probe = mqtt_broker::Probe::start(broker.port).await;
        let _process = Process(
            std::process::Command::new(env!("CARGO_BIN_EXE_homecore-xiaomi"))
                .env("MQTT_HOST", "127.0.0.1")
                .env("MQTT_PORT", broker.port.to_string())
                .env("MQTT_USERNAME", "fixture-user")
                .env("MQTT_PASSWORD_FILE", &passwords)
                .env("XIAOMI_STATE_DIR", &root)
                .env("XIAOMI_UI_DIR", "ui")
                .env("XIAOMI_DATABASE", backend)
                .envs(extra_env.iter().map(|(key, value)| (*key, value)))
                .spawn()
                .unwrap(),
        );
        let reply = tokio::time::timeout(Duration::from_secs(10), async {
            while probe.messages.recv().await.unwrap().topic
                != "homecore/plugins/fixture-user/capabilities"
            {}
            probe
                .client
                .publish(
                    "homecore/plugins/fixture-user/manage/cmd",
                    QoS::AtLeastOnce,
                    false,
                    json!({"action":"sign_in_status", "request_id":"stored_adapters"}).to_string(),
                )
                .await
                .unwrap();
            loop {
                let message = probe.messages.recv().await.unwrap();
                if message.topic == "homecore/plugins/fixture-user/manage/response" {
                    let value: Value = serde_json::from_slice(&message.payload).unwrap();
                    if value["request_id"] == "stored_adapters" {
                        break value;
                    }
                }
            }
        })
        .await
        .expect("Xiaomi setup status should reply");
        results.push(reply["registered_adapters"].clone());
        if attempt == 0 {
            fs::remove_file(&source).unwrap();
        }
    }

    assert_eq!(results, vec![registrations.clone(), registrations]);
}

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

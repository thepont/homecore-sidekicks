#[path = "../../teslemetry/tests/support/homecore.rs"]
mod homecore;
use serde_json::{json, Value};
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Service(Child);
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
#[ignore = "requires Docker and the pinned homeCore image"]
async fn core_serves_the_unconfigured_setup_page_to_an_authenticated_user() {
    let core = homecore::Core::start().await;
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let _service = Service(
        Command::new(env!("CARGO_BIN_EXE_homecore-xiaomi"))
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", core.mqtt_port.to_string())
            .env("MQTT_USERNAME", "plugin.teslemetry-powerwall")
            .env("MQTT_PASSWORD_FILE", core.root.join("mqtt-password"))
            .env("XIAOMI_STATE_DIR", &core.root)
            .env("XIAOMI_UI_DIR", &ui)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let entry = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let list: Value = core
                .get("/plugins/ui")
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if let Some(item) = list.as_array().and_then(|xs| {
                xs.iter()
                    .find(|x| x["plugin_id"] == "plugin.teslemetry-powerwall")
            }) {
                break item["ui"].clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("setup page should be advertised");
    assert!(entry["slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["slot"] == "plugin.page" && s["element"] == "hc-xiaomi-setup"));
    let path = format!(
        "/plugins/plugin.teslemetry-powerwall/ui/{}/{}",
        entry["build"].as_str().unwrap(),
        entry["entry"].as_str().unwrap()
    );
    assert_eq!(
        core.anonymous_get(&path).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let response = core.get(&path).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        std::fs::read(ui.join("index.js")).unwrap()
    );
    let commands = "/plugins/plugin.teslemetry-powerwall/command";
    for body in [
        json!({"action":"register_adapter","plugin_id":"plugin.fixture-vacuum","models":["ijai.vacuum.v10"]}),
        json!({"action":"request_access","plugin_id":"plugin.fixture-vacuum","model":"ijai.vacuum.v10"}),
    ] {
        let reply: Value = core
            .post(commands)
            .json(&body)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["status"], "ok", "{reply}");
    }
    let state: Value = core
        .post(commands)
        .json(&json!({"action":"sign_in_status"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        state["access_requests"],
        json!([
            {"plugin_id":"plugin.fixture-vacuum","model":"ijai.vacuum.v10","status":"pending"}
        ])
    );
}

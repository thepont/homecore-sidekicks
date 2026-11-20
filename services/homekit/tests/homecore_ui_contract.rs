#[path = "../../teslemetry/tests/support/homecore.rs"]
#[allow(dead_code)]
mod homecore;
use serde_json::Value;
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
async fn authenticated_homecore_offers_the_unconfigured_setup_screen() {
    let core = homecore::Core::start().await;
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let state = core.root.join("homekit");
    std::fs::create_dir(&state).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _service = Service(
        Command::new(env!("CARGO_BIN_EXE_homecore-homekit"))
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", core.mqtt_port.to_string())
            .env("MQTT_USERNAME", "plugin.teslemetry-powerwall")
            .env("MQTT_PASSWORD_FILE", core.root.join("mqtt-password"))
            .env("HOMEKIT_PLUGIN_ID", "plugin.teslemetry-powerwall")
            .env("HOMEKIT_STATE_DIR", &state)
            .env("HOMEKIT_UI_DIR", &ui)
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
    .unwrap();
    assert!(entry["slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["slot"] == "plugin.page" && s["element"] == "hc-homekit-setup"));
    let path = format!(
        "/plugins/plugin.teslemetry-powerwall/ui/{}/{}",
        entry["build"].as_str().unwrap(),
        entry["entry"].as_str().unwrap()
    );
    assert_eq!(
        core.anonymous_get(&path).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        core.get(&path)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        std::fs::read(ui.join("index.js")).unwrap()
    );
}

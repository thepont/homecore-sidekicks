use homecore_test_support::homecore;
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
async fn homecore_managed_config_starts_setup_with_private_persistent_state() {
    let plugin = "plugin.homekit";
    let core = homecore::Core::start_for_plugin(plugin).await;
    let config = core.root.join("plugin.homekit.toml");
    let password = std::fs::read_to_string(core.root.join("mqtt-password")).unwrap();
    std::fs::write(
        &config,
        format!(
            "[homecore]\nbroker_host = '127.0.0.1'\nbroker_port = {}\nplugin_id = '{plugin}'\npassword = {:?}\n[homekit]\nrefresh_secs = 45\n",
            core.mqtt_port, password.trim()
        ),
    )
    .unwrap();
    let mut service = Service(
        Command::new(env!("CARGO_BIN_EXE_homecore-homekit"))
            .env_clear()
            .env(
                "HOMEKIT_UI_DIR",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"),
            )
            .arg(&config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let entry = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            assert!(
                service.0.try_wait().unwrap().is_none(),
                "managed bridge exited during setup"
            );
            if let Some(entry) = find_ui(&core, plugin).await {
                break entry;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("managed HomeKit setup is offered");
    assert_eq!(entry["slots"][0]["element"], "hc-homekit-setup");
    use std::os::unix::fs::PermissionsExt;
    let state = std::fs::metadata(config.with_extension("state")).unwrap();
    assert!(state.is_dir());
    assert_eq!(state.permissions().mode() & 0o777, 0o700);
    let schema: Value = core
        .get("/plugins/plugin.homekit/config/schema")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        schema["schema"]["properties"]["refresh_secs"]["default"],
        30
    );
}

async fn find_ui(core: &homecore::Core, plugin: &str) -> Option<Value> {
    let list: Value = core
        .get("/plugins/ui")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    list.as_array()?
        .iter()
        .find(|x| x["plugin_id"] == plugin)
        .map(|x| x["ui"].clone())
}

#[tokio::test]
#[ignore = "requires Docker and HOMEKIT_TEST_ARCHIVE"]
async fn installs_the_signed_homekit_package_and_serves_its_setup_page() {
    let id = "plugin.homekit";
    let archive = std::env::var_os("HOMEKIT_TEST_ARCHIVE").expect("set HOMEKIT_TEST_ARCHIVE");
    let core = homecore::Core::start_with_registry(homecore::RegistryPackage {
        archive: std::path::Path::new(&archive),
        id,
        version: env!("CARGO_PKG_VERSION"),
    })
    .await;
    let response = core
        .post("/plugins/install")
        .json(&serde_json::json!({"id":id}))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "install failed with {}",
        response.status()
    );
    let entry = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(entry) = find_ui(&core, id).await {
                break entry;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("installed package offers HomeKit setup");
    assert_eq!(entry["slots"][0]["element"], "hc-homekit-setup");
    let path = format!(
        "/plugins/{id}/ui/{}/index.js",
        entry["build"].as_str().unwrap()
    );
    assert_eq!(
        core.anonymous_get(&path).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let bundle = core
        .get(&path)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/index.js");
    assert_eq!(bundle.as_ref(), std::fs::read(expected).unwrap());
}

#[tokio::test]
#[ignore = "requires Docker and the pinned homeCore image"]
async fn authenticated_homecore_offers_the_unconfigured_setup_screen() {
    let core = homecore::Core::start_for_plugin("plugin.teslemetry-powerwall").await;
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

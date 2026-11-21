use homecore_xiaomi::{actions::dispatch_with_params, login::Provider, setup::Setup, store::Store};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct State(PathBuf);

impl Drop for State {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl State {
    async fn setup(&self) -> Setup {
        Setup::new(
            Provider::new("http://127.0.0.1:1").unwrap(),
            Store::new(&self.0),
            "de",
        )
        .with_sqlite(&self.0.join("state.sqlite3"))
        .await
        .unwrap()
    }
}

async fn action(setup: &Setup, name: &str, params: Value) -> Value {
    dispatch_with_params(setup, name, &params)
        .await
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn an_installer_can_register_an_adapter_and_request_access_without_signing_in() {
    let root = std::env::temp_dir().join(format!("xiaomi-access-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let state = State(root);
    fs::write(
        state.0.join("adapters.json"),
        br#"[{"plugin_id":"plugin.xiaomi-gateway","models":["lumi.gateway.mcn001"]}]"#,
    )
    .unwrap();
    let setup = state.setup().await;
    let registration = json!({"plugin_id":"plugin.xiaomi-vacuum","models":["ijai.vacuum.v10"]});
    assert_eq!(
        action(&setup, "register_adapter", registration.clone()).await["status"],
        "ok"
    );
    let request = json!({"plugin_id":"plugin.xiaomi-vacuum","model":"ijai.vacuum.v10"});
    for _ in 0..2 {
        assert_eq!(
            action(&setup, "request_access", request.clone()).await["status"],
            "ok"
        );
    }
    let outside_registration = json!({
        "plugin_id":"plugin.xiaomi-vacuum","model":"lumi.gateway.mcn001"
    });
    assert_eq!(
        action(&setup, "request_access", outside_registration).await["status"],
        "error"
    );
    drop(setup);

    let restarted = state.setup().await;
    let status = action(&restarted, "sign_in_status", json!({})).await;
    assert_eq!(status["setup_status"], "unconfigured");
    assert_eq!(
        status["registered_adapters"],
        json!([
            {"plugin_id":"plugin.xiaomi-gateway","models":["lumi.gateway.mcn001"]},
            registration
        ])
    );
    assert_eq!(
        status["access_requests"],
        json!([
            {"plugin_id":"plugin.xiaomi-vacuum","model":"ijai.vacuum.v10","status":"pending"}
        ])
    );
}

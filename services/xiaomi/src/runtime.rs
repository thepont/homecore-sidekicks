use crate::{actions, setup::Setup};
use plugin_sdk_rs::{types::Capabilities, AsyncActionFuture, PluginClient, PluginConfig};
use serde_json::json;
use std::path::PathBuf;

pub async fn run(
    host: String,
    port: u16,
    id: String,
    password: String,
    setup: Setup,
    ui: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = PluginClient::connect(PluginConfig {
        broker_host: host,
        broker_port: port,
        plugin_id: id,
        password,
    })
    .await?;
    let caps: Capabilities = serde_json::from_value(
        json!({"spec":"1", "plugin_id":"", "actions":[
        {"id":"begin_sign_in", "label":"Sign in with Mi Home", "requires_role":"admin", "timeout_ms":25000, "params":{"region":{"type":"string", "enum":["cn","de","i2","ru","sg","us"]}}},
            {"id":"sign_in_status", "label":"Check Xiaomi setup", "requires_role":"admin", "timeout_ms":5000}
        ]}),
    )?;
    let mgmt = client
        .enable_management(30, Some(env!("CARGO_PKG_VERSION").into()), None, None)
        .await?
        .with_capabilities(caps)
        .with_ui_dir(ui, env!("CARGO_PKG_VERSION"))
        .with_async_handler(move |command| {
            let action = command.get("action")?.as_str()?;
            if action != "begin_sign_in" && action != "sign_in_status" {
                return None;
            }
            let action = action.to_owned();
            let setup = setup.clone();
            let params = command.clone();
            Some(Box::pin(async move {
                actions::dispatch_with_params(&setup, &action, &params)
                    .await
                    .unwrap_or_else(|| json!({"status":"error"}))
            }) as AsyncActionFuture)
        });
    client.run_managed(|_, _| {}, mgmt).await?;
    Ok(())
}

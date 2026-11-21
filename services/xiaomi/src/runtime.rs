use crate::{actions::Command, setup::Setup};
use plugin_sdk_rs::{
    types::{Capabilities, PluginNotice},
    AsyncActionFuture, PluginClient, PluginConfig, PluginNotices,
};
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
    let notices = client.notices();
    sync_access_notice(&notices, &setup).await;
    let caps: Capabilities = serde_json::from_value(
        json!({"spec":"1", "plugin_id":"", "actions":[
        {"id":"begin_sign_in", "label":"Sign in with Mi Home", "requires_role":"admin", "timeout_ms":25000, "params":{"region":{"type":"string", "enum":["cn","de","i2","ru","sg","us"]}}},
            {"id":"sign_in_status", "label":"Check Xiaomi setup", "requires_role":"admin", "timeout_ms":5000},
            {"id":"register_adapter", "label":"Register Xiaomi adapter", "requires_role":"admin", "timeout_ms":10000, "params":{"plugin_id":{"type":"string","required":true},"models":{"type":"array","required":true}}},
            {"id":"request_access", "label":"Request Xiaomi device access", "requires_role":"admin", "timeout_ms":10000, "params":{"plugin_id":{"type":"string","required":true},"model":{"type":"string","required":true}}},
            {"id":"approve_access", "label":"Approve Xiaomi device access", "requires_role":"admin", "timeout_ms":10000, "params":{"plugin_id":{"type":"string","required":true},"device_ref":{"type":"string","required":true}}},
            {"id":"deny_access", "label":"Deny Xiaomi device access", "requires_role":"admin", "timeout_ms":10000, "params":{"plugin_id":{"type":"string","required":true},"model":{"type":"string","required":true}}},
            {"id":"revoke_access", "label":"Revoke Xiaomi device access", "requires_role":"admin", "timeout_ms":10000, "params":{"plugin_id":{"type":"string","required":true},"device_ref":{"type":"string","required":true}}}
        ]}),
    )?;
    let mgmt = client
        .enable_management(30, Some(env!("CARGO_PKG_VERSION").into()), None, None)
        .await?
        .with_capabilities(caps)
        .with_ui_dir(ui, env!("CARGO_PKG_VERSION"))
        .with_async_handler(move |command| {
            let action = Command::from_id(command.get("action")?.as_str()?)?;
            let setup = setup.clone();
            let params = command.clone();
            let notices = notices.clone();
            Some(Box::pin(async move {
                let updates_requests = matches!(
                    action,
                    Command::RequestAccess | Command::ApproveAccess | Command::DenyAccess
                );
                let result = action.execute(&setup, &params).await;
                if updates_requests && result["status"] == "ok" {
                    sync_access_notice(&notices, &setup).await;
                }
                result
            }) as AsyncActionFuture)
        });
    client.run_managed(|_, _| {}, mgmt).await?;
    Ok(())
}

async fn sync_access_notice(notices: &PluginNotices, setup: &Setup) {
    let code = "xiaomi_access_pending";
    if setup.has_pending_requests().await {
        notices.raise(PluginNotice::warning(
            code,
            "Xiaomi device access is awaiting approval.",
        ));
    } else {
        notices.clear(code);
    }
}

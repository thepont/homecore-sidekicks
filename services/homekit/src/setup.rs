use plugin_sdk_rs::{types::Capabilities, AsyncActionFuture, PluginClient, PluginConfig};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;
pub type Discover = Arc<
    dyn Fn() -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<hap_transport::DiscoveredAccessory>, ()>,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

pub struct ManagementConfig {
    pub host: String,
    pub port: u16,
    pub id: String,
    pub password: String,
    pub state_dir: PathBuf,
    pub ui: PathBuf,
}

/// Serve authenticated homeCore management requests for an unconfigured controller.
pub async fn run(
    host: String,
    port: u16,
    id: String,
    password: String,
    state_dir: PathBuf,
    ui: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_with_discovery(
        host,
        port,
        id,
        password,
        state_dir,
        ui,
        Arc::new(|| {
            Box::pin(async {
                hap_transport::discover(Duration::from_secs(3))
                    .await
                    .map_err(|_| ())
            })
        }),
    )
    .await
}

/// Serve management using an injected discovery source.
pub async fn run_with_discovery(
    host: String,
    port: u16,
    id: String,
    password: String,
    state_dir: PathBuf,
    ui: PathBuf,
    discover: Discover,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_inner(
        ManagementConfig {
            host,
            port,
            id,
            password,
            state_dir,
            ui,
        },
        discover,
        30,
        None,
    )
    .await
}

/// Serve management and hand the shared device tracker to the publisher.
pub async fn run_with_tracking(
    config: ManagementConfig,
    heartbeat: u64,
    tracker: tokio::sync::oneshot::Sender<plugin_sdk_rs::DevicePublisher>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_inner(
        config,
        Arc::new(|| {
            Box::pin(async {
                hap_transport::discover(Duration::from_secs(3))
                    .await
                    .map_err(|_| ())
            })
        }),
        heartbeat,
        Some(tracker),
    )
    .await
}

async fn run_inner(
    config: ManagementConfig,
    discover: Discover,
    heartbeat: u64,
    tracker: Option<tokio::sync::oneshot::Sender<plugin_sdk_rs::DevicePublisher>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ManagementConfig {
        host,
        port,
        id,
        password,
        state_dir,
        ui,
    } = config;
    let client = PluginClient::connect(PluginConfig {
        broker_host: host,
        broker_port: port,
        plugin_id: id,
        password,
    })
    .await?;
    if let Some(tracker) = tracker {
        let _ = tracker.send(client.device_publisher());
    }
    let caps: Capabilities = serde_json::from_value(json!({"spec":"1","plugin_id":"","actions":[
        {"id":"discover_accessories","label":"Find HomeKit accessories","requires_role":"admin","timeout_ms":8000},
        {"id":"pair_accessory","label":"Connect HomeKit accessory","requires_role":"admin","timeout_ms":45000,"params":{"id":{"type":"string"},"pin":{"type":"string"}}}
    ]}))?;
    let discoveries = Arc::new(Mutex::new(Vec::<hap_transport::DiscoveredAccessory>::new()));
    let management = client
        .enable_management(
            heartbeat,
            Some(env!("CARGO_PKG_VERSION").into()),
            None,
            None,
        )
        .await?
        .with_capabilities(caps)
        .with_config_schema(json!({"type":"object","properties":{
            "refresh_secs":{"type":"integer","minimum":1,"maximum":3600,"default":30}
        }}))
        .with_ui_dir(ui, env!("CARGO_PKG_VERSION"))
        .with_async_handler(move |command| {
            let action = command.get("action")?.as_str()?;
            if action != "discover_accessories" && action != "pair_accessory" {
                return None;
            }
            let action = action.to_owned();
            let params = command.clone();
            let discoveries = discoveries.clone();
            let state_dir = state_dir.clone();
            let discover = discover.clone();
            Some(Box::pin(async move {
                if action == "discover_accessories" {
                    match discover().await {
                        Ok(found) => {
                            let found: Vec<_> = found
                                .into_iter()
                                .filter(|a| a.id.len() == 17 && a.name.len() <= 128)
                                .take(100)
                                .collect();
                            let items: Vec<_> = found
                                .iter()
                                .map(|a| json!({"id":a.id,"name":a.name,"paired":a.paired}))
                                .collect();
                            *discoveries.lock().await = found;
                            json!({"status":"ok","accessories":items})
                        }
                        Err(_) => json!({"status":"error","error":"Discovery failed"}),
                    }
                } else {
                    let (Some(id), Some(pin)) = (
                        params.get("id").and_then(|v| v.as_str()),
                        params.get("pin").and_then(|v| v.as_str()),
                    ) else {
                        return json!({"status":"error","error":"Invalid pairing details"});
                    };
                    let found: Vec<_> = discoveries
                        .lock()
                        .await
                        .iter()
                        .filter(|a| a.id.eq_ignore_ascii_case(id) && !a.paired)
                        .cloned()
                        .collect();
                    if found.len() != 1 {
                        return json!({"status":"error","error":"Accessory unavailable"});
                    }
                    let accessory = found[0].clone();
                    let pin = pin.to_owned();
                    let result = tokio::task::spawn_blocking(move || {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .map_err(|_| "Cannot start pairing")?;
                        runtime.block_on(async {
                            tokio::time::timeout(
                                Duration::from_secs(38),
                                crate::pair_and_read(
                                    hap_controller::Discovered::Ip(accessory),
                                    &pin,
                                    &state_dir,
                                ),
                            )
                            .await
                            .map_err(|_| {
                                "Pairing confirmation was lost; accessory recovery may be needed"
                            })?
                        })
                    })
                    .await;
                    match result {
                        Ok(Ok(_)) => json!({"status":"ok"}),
                        Ok(Err("Controller already configured")) => {
                            json!({"status":"error","error":"Controller already configured"})
                        }
                        Ok(Err(error))
                            if error.contains("recovery")
                                || error.contains("confirmation was lost") =>
                        {
                            json!({"status":"recovery_required"})
                        }
                        _ => json!({"status":"error","error":"Pairing failed"}),
                    }
                }
            }) as AsyncActionFuture)
        });
    client.run_managed(|_, _| {}, management).await?;
    Ok(())
}

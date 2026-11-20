//! homeCore's management protocol shares the telemetry MQTT connection.
//! One queued request and one active disk read bound UI work under load.
use crate::ui::{self, UiAssets, MAX_REQUEST_BYTES};
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, QoS};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};

const PREFIX: &str = "homecore/plugins/plugin.teslemetry-powerwall";
const COMMAND: &str = "homecore/plugins/plugin.teslemetry-powerwall/manage/cmd";
const SEND_DEADLINE: Duration = Duration::from_secs(2);

fn assets() -> Option<UiAssets> {
    let dir = std::env::var_os("HC_PLUGIN_UI_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(|p| p.join("ui")))?;
    match UiAssets::load(&dir, env!("CARGO_PKG_VERSION")) {
        Ok(assets) => Some(assets),
        Err(_) => {
            eprintln!("Card assets unavailable; telemetry will continue");
            None
        }
    }
}

pub async fn run(client: AsyncClient, mut events: EventLoop, connections: watch::Sender<()>) {
    let (online_tx, online_rx) = watch::channel(false);
    let (requests_tx, requests_rx) = mpsc::channel(1);
    let worker = tokio::spawn(serve(client, online_rx, requests_rx, assets()));
    loop {
        match events.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                online_tx.send_replace(true);
                connections.send_replace(());
            }
            Ok(Event::Incoming(Incoming::Publish(p)))
                if p.topic == COMMAND && !p.retain && p.payload.len() <= MAX_REQUEST_BYTES =>
            {
                // Saturated requests time out at homeCore; MQTT polling never
                // waits on disk work. Replies share bounded telemetry backpressure.
                let _ = requests_tx.try_send(p.payload.to_vec());
            }
            Ok(_) => {}
            Err(_) => {
                online_tx.send_replace(false);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        if worker.is_finished() {
            // A broken management worker must not leave telemetry apparently healthy.
            eprintln!("Plugin management stopped");
            return;
        }
    }
}

async fn send(client: &AsyncClient, suffix: &str, qos: QoS, retain: bool, body: serde_json::Value) {
    if let Ok(bytes) = serde_json::to_vec(&body) {
        let _ = tokio::time::timeout(
            SEND_DEADLINE,
            client.publish(format!("{PREFIX}/{suffix}"), qos, retain, bytes),
        )
        .await;
    }
}

async fn serve(
    client: AsyncClient,
    mut online: watch::Receiver<bool>,
    mut requests: mpsc::Receiver<Vec<u8>>,
    assets: Option<UiAssets>,
) {
    let assets = Arc::new(assets);
    let started = Instant::now();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            changed = online.changed() => {
                if changed.is_err() { return; }
                if *online.borrow_and_update() {
                    let _ = tokio::time::timeout(SEND_DEADLINE,
                        client.subscribe(COMMAND, QoS::AtLeastOnce)).await;
                    send(&client, "capabilities", QoS::AtLeastOnce, true, json!({
                        "spec":"1", "plugin_id":"plugin.teslemetry-powerwall", "actions":[],
                        "ui": assets.as_ref().as_ref().map(UiAssets::descriptor)
                    })).await;
                    heartbeat.reset_immediately();
                }
            }
            _ = heartbeat.tick(), if *online.borrow() => {
                send(&client, "heartbeat", QoS::AtMostOnce, false, json!({
                    "version":env!("CARGO_PKG_VERSION"), "protocol_version":"0.1.5",
                    "uptime_secs":started.elapsed().as_secs(), "notices":[]
                })).await;
            }
            request = requests.recv() => {
                let Some(request) = request else { return; };
                if !*online.borrow() { continue; }
                let assets = Arc::clone(&assets);
                let reply = tokio::task::spawn_blocking(move || match assets.as_ref() {
                    Some(assets) => assets.respond(&request),
                    None => ui::respond_unavailable(&request),
                }).await;
                if let Ok(Some(reply)) = reply {
                    send(&client, "manage/response", QoS::AtLeastOnce, false, reply).await;
                }
            }
        }
    }
}

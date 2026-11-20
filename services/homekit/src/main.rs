use rumqttc::{AsyncClient, Event, Incoming, MqttOptions};
use std::{env, io::Read, path::PathBuf, time::Duration};
use tokio::sync::watch;

fn required(name: &str) -> Result<String, &'static str> {
    env::var(name)
        .ok()
        .filter(|v| !v.is_empty() && v.len() <= 4096)
        .ok_or("Invalid configuration")
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("HomeKit service failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), &'static str> {
    let host = required("MQTT_HOST")?;
    let port = required("MQTT_PORT")?
        .parse::<u16>()
        .map_err(|_| "Invalid broker port")?;
    if !matches!(host.as_str(), "127.0.0.1" | "::1") || port == 0 {
        return Err("Invalid broker address");
    }
    let username = required("MQTT_USERNAME")?;
    if username.len() > 128 || !username.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Invalid broker user");
    }
    let mut password = String::new();
    std::fs::File::open(required("MQTT_PASSWORD_FILE")?)
        .map_err(|_| "Cannot read broker credential")?
        .take(4097)
        .read_to_string(&mut password)
        .map_err(|_| "Cannot read broker credential")?;
    let password = password.trim_end_matches(['\r', '\n']);
    if password.is_empty()
        || password.len() > 4096
        || !password.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err("Invalid broker credential");
    }
    let plugin = required("HOMEKIT_PLUGIN_ID")?;
    let bridge_id = env::var("HOMEKIT_BRIDGE_ID").ok().filter(|v| !v.is_empty());
    let state_dir = PathBuf::from(required("HOMEKIT_STATE_DIR")?);
    let ui = PathBuf::from(env::var("HOMEKIT_UI_DIR").unwrap_or_else(|_| "ui".into()));
    let refresh = env::var("HOMEKIT_REFRESH_SECS")
        .unwrap_or_else(|_| "30".into())
        .parse::<u64>()
        .map_err(|_| "Invalid refresh interval")?;
    if !(1..=3600).contains(&refresh) {
        return Err("Invalid refresh interval");
    }
    let management = homecore_homekit::setup::run(
        host.clone(),
        port,
        username.clone(),
        password.to_owned(),
        state_dir.clone(),
        ui,
    );
    let mut options = MqttOptions::new(format!("{plugin}-publisher"), host, port);
    options.set_credentials(username, password);
    options.set_keep_alive(Duration::from_secs(15));
    let availability_options = options.clone();
    let (mqtt, mut events) = AsyncClient::new(options, 32);
    let (ready, mut connection) = watch::channel(false);
    let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(32);
    let worker = tokio::spawn(async move {
        loop {
            match events.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    ready.send_replace(true);
                }
                Ok(Event::Incoming(Incoming::Publish(message))) => {
                    if !message.retain && message.payload.len() <= 4096 {
                        if let Ok(value) = serde_json::from_slice(&message.payload) {
                            let _ = command_tx.try_send((message.topic, value));
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    ready.send_replace(false);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
    let publisher = async move {
        loop {
            while !*connection.borrow_and_update() {
                if connection.changed().await.is_err() {
                    worker.abort();
                    return Err("Broker worker stopped");
                }
            }
            let configured_id = match &bridge_id {
                Some(id) => Some(id.clone()),
                None => homecore_homekit::stored_bridge_id(&state_dir)
                    .await
                    .ok()
                    .flatten(),
            };
            let Some(bridge_id) = configured_id else {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            };
            tokio::select! {
            _ = homecore_homekit::run_sensor_publisher_with_commands(mqtt.clone(), plugin.clone(), bridge_id, state_dir.clone(), Duration::from_secs(refresh), &mut command_rx, Some(availability_options.clone())) => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                _ = connection.changed() => {}
            }
        }
    };
    tokio::select! {
        result = management => result.map_err(|_| "Setup service stopped"),
        result = publisher => result,
    }
}

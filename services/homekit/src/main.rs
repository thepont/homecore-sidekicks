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
    let bridge_id = required("HOMEKIT_BRIDGE_ID")?;
    let state_dir = PathBuf::from(required("HOMEKIT_STATE_DIR")?);
    let mut options = MqttOptions::new(&plugin, host, port);
    options.set_credentials(username, password);
    options.set_keep_alive(Duration::from_secs(15));
    let (mqtt, mut events) = AsyncClient::new(options, 32);
    let (ready, mut connection) = watch::channel(false);
    let worker = tokio::spawn(async move {
        loop {
            match events.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    ready.send_replace(true);
                }
                Ok(_) => {}
                Err(_) => {
                    ready.send_replace(false);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
    loop {
        while !*connection.borrow_and_update() {
            if connection.changed().await.is_err() {
                worker.abort();
                return Err("Broker worker stopped");
            }
        }
        tokio::select! {
            _ = homecore_homekit::run_contact_publisher(mqtt.clone(), plugin.clone(), bridge_id.clone(), state_dir.clone()) => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            _ = connection.changed() => {}
        }
    }
}

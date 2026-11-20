use rumqttc::{AsyncClient, Event, Incoming, MqttOptions};
use serde::Deserialize;
use std::{
    env, fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::watch;

const PLUGIN_ID: &str = "plugin.homekit";

struct Settings {
    host: String,
    port: u16,
    username: String,
    password: String,
    plugin: String,
    bridge_id: Option<String>,
    state_dir: PathBuf,
    ui: PathBuf,
    refresh: u64,
}

#[derive(Deserialize)]
struct Config {
    homecore: Broker,
    #[serde(default)]
    homekit: Controller,
}

#[derive(Deserialize)]
struct Broker {
    broker_host: String,
    broker_port: u16,
    plugin_id: String,
    #[serde(default)]
    password: String,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Controller {
    refresh_secs: u64,
}

impl Default for Controller {
    fn default() -> Self {
        Self { refresh_secs: 30 }
    }
}

fn managed_settings(path: &Path) -> Result<Settings, &'static str> {
    let mut raw = String::new();
    fs::File::open(path)
        .map_err(|_| "Cannot open HomeCore configuration")?
        .take(65_537)
        .read_to_string(&mut raw)
        .map_err(|_| "Cannot read HomeCore configuration")?;
    if raw.len() > 65_536 {
        return Err("Invalid HomeCore configuration");
    }
    let Config { homecore, homekit } =
        toml::from_str(&raw).map_err(|_| "Invalid HomeCore configuration")?;
    if homecore.plugin_id != PLUGIN_ID {
        return Err("Invalid HomeKit plugin identity");
    }
    let host = match homecore.broker_host.as_str() {
        "127.0.0.1" | "0.0.0.0" => "127.0.0.1",
        "::1" | "::" => "::1",
        _ => return Err("Invalid broker address"),
    };
    if homecore.broker_port == 0 || !(1..=3600).contains(&homekit.refresh_secs) {
        return Err("Invalid broker port or refresh interval");
    }
    if homecore.password.len() > 4096 || !homecore.password.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Invalid broker credential");
    }
    let ui = env::var_os("HOMEKIT_UI_DIR")
        .map(PathBuf::from)
        .or_else(|| plugin_sdk_rs::ui::find_dir(env!("CARGO_MANIFEST_DIR")))
        .ok_or("HomeKit UI bundle is unavailable")?;
    let state_dir = path.with_extension("state");
    match fs::DirBuilder::new().mode(0o700).create(&state_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Cannot create pairing directory"),
    }
    let metadata = fs::symlink_metadata(&state_dir).map_err(|_| "Invalid pairing directory")?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Invalid pairing directory");
    }
    Ok(Settings {
        host: host.into(),
        port: homecore.broker_port,
        username: homecore.plugin_id.clone(),
        password: homecore.password,
        plugin: homecore.plugin_id,
        bridge_id: None,
        state_dir,
        ui,
        refresh: homekit.refresh_secs,
    })
}

fn required(name: &str) -> Result<String, &'static str> {
    env::var(name)
        .ok()
        .filter(|v| !v.is_empty() && v.len() <= 4096)
        .ok_or("Invalid configuration")
}

fn legacy_settings() -> Result<Settings, &'static str> {
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
    Ok(Settings {
        host,
        port,
        username,
        password: password.to_owned(),
        plugin,
        bridge_id,
        state_dir,
        ui,
        refresh,
    })
}

#[tokio::main]
async fn main() {
    let settings = match env::args_os().nth(1) {
        Some(path) => managed_settings(Path::new(&path)),
        None => legacy_settings(),
    };
    let result = match settings {
        Ok(settings) => run(settings).await,
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("HomeKit service failed: {error}");
        std::process::exit(1);
    }
}

async fn run(settings: Settings) -> Result<(), &'static str> {
    let Settings {
        host,
        port,
        username,
        password,
        plugin,
        bridge_id,
        state_dir,
        ui,
        refresh,
    } = settings;
    let management = homecore_homekit::setup::run(
        host.clone(),
        port,
        username.clone(),
        password.clone(),
        state_dir.clone(),
        ui,
    );
    let mut options = MqttOptions::new(format!("{plugin}-publisher"), host, port);
    if !password.is_empty() {
        options.set_credentials(username, password);
    }
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
            _ = homecore_homekit::run_contact_publisher_with_interval(mqtt.clone(), plugin.clone(), bridge_id, state_dir.clone(), Duration::from_secs(refresh)) => {
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

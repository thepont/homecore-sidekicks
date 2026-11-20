use homecore_electrolux::transport::Client;
use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, QoS};
use serde_json::json;
use std::{env, fs::File, io::Read, net::SocketAddr, time::Duration};
use tokio::sync::watch;

const DEVICE: &str = "electrolux_living_room";
const PLUGIN: &str = "plugin.electrolux.living_room";
const AVAIL: &str = "homecore/devices/electrolux_living_room/availability";

enum Command {
    Temperature(u8),
    Power(bool),
    Mode(&'static str),
    Fan(&'static str),
    Swing(bool),
}
fn command(payload: &[u8]) -> Option<Command> {
    let serde_json::Value::Object(values) = serde_json::from_slice(payload).ok()? else {
        return None;
    };
    if values.get("_hc").is_some_and(|v| !v.is_object())
        || values.get("correlation_id").is_some_and(|v| {
            !v.as_str().is_some_and(|s| {
                !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_graphic())
            })
        })
    {
        return None;
    }
    let mut controls = values
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "_hc" | "correlation_id"));
    let (key, value) = controls.next()?;
    if controls.next().is_some() {
        return None;
    }
    match key.as_str() {
        "setpoint" => value
            .as_f64()
            .filter(|v| v.fract() == 0.0 && (17.0..=30.0).contains(v))
            .map(|v| Command::Temperature(v as u8)),
        "on" => value.as_bool().map(Command::Power),
        "swing" => value.as_bool().map(Command::Swing),
        "vertical_vane" => match value.as_str()? {
            "swing" => Some(Command::Swing(true)),
            "off" => Some(Command::Swing(false)),
            _ => None,
        },
        "hvac_mode" => match value.as_str()? {
            "off" => Some(Command::Power(false)),
            "cool" => Some(Command::Mode("cool")),
            "heat" => Some(Command::Mode("heat")),
            "dry" => Some(Command::Mode("dry")),
            "fan_only" => Some(Command::Mode("fan_only")),
            "auto" => Some(Command::Mode("auto")),
            "heat_cool" => Some(Command::Mode("heat_cool")),
            _ => None,
        },
        "fan_mode" => match value.as_str()? {
            "auto" => Some(Command::Fan("auto")),
            "low" => Some(Command::Fan("low")),
            "medium" => Some(Command::Fan("medium")),
            "high" => Some(Command::Fan("high")),
            "turbo" => Some(Command::Fan("turbo")),
            "quiet" => Some(Command::Fan("quiet")),
            _ => None,
        },
        _ => None,
    }
}

fn setting(name: &str) -> Result<String, &'static str> {
    env::var(name).map_err(|_| "Missing configuration")
}
fn mac(raw: &str) -> Result<[u8; 6], &'static str> {
    if raw.len() != 12 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid MAC");
    }
    let mut value = [0; 6];
    for (i, byte) in value.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16).map_err(|_| "Invalid MAC")?;
    }
    Ok(value)
}
fn secret(path: &str) -> Result<String, &'static str> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "Cannot read credential")?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read credential")?;
    if bytes.len() > 4096 {
        return Err("Invalid credential");
    }
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| "Invalid credential")?
        .trim();
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Invalid credential");
    }
    Ok(value.to_owned())
}
#[tokio::main]
async fn main() -> Result<(), &'static str> {
    let address: SocketAddr = setting("ELECTROLUX_ADDRESS")?
        .parse()
        .map_err(|_| "Invalid address")?;
    if !address.is_ipv4() || address.port() == 0 {
        return Err("Invalid address");
    }
    let mac = mac(&setting("ELECTROLUX_MAC")?)?;
    let host = setting("MQTT_HOST")?;
    let port: u16 = setting("MQTT_PORT")?
        .parse()
        .map_err(|_| "Invalid broker")?;
    if host != "127.0.0.1" || port == 0 {
        return Err("Invalid broker");
    }
    let username = setting("MQTT_USERNAME")?;
    if username.is_empty()
        || username.len() > 128
        || !username.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err("Invalid broker username");
    }
    let password = secret(&setting("MQTT_PASSWORD_FILE")?)?;
    let mut opts = MqttOptions::new(PLUGIN, host, port);
    opts.set_credentials(username, password);
    opts.set_keep_alive(Duration::from_secs(15));
    // UI file replies may carry a 128 KiB chunk encoded as base64.
    opts.set_max_packet_size(8 * 1024, 256 * 1024);
    opts.set_inflight(2);
    opts.set_last_will(LastWill::new(AVAIL, "offline", QoS::AtLeastOnce, true));
    let (mqtt, mut events) = AsyncClient::new(opts, 16);
    let assets = std::sync::Arc::new(homecore_electrolux::ui::assets());
    let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1);
    let ui_mqtt = mqtt.clone();
    let ui_assets = assets.clone();
    tokio::spawn(async move {
        while let Some(request) = ui_rx.recv().await {
            let assets = ui_assets.clone();
            let reply = tokio::task::spawn_blocking(move || match assets.as_ref() {
                Some(assets) => assets.respond(&request),
                None => homecore_plugin_ui::respond_unavailable(&request),
            })
            .await;
            if let Ok(Some(reply)) = reply {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    ui_mqtt.publish(
                        format!("homecore/plugins/{PLUGIN}/manage/response"),
                        QoS::AtLeastOnce,
                        false,
                        reply.to_string(),
                    ),
                )
                .await;
            }
        }
    });
    let (tx, mut rx) = watch::channel(None);
    let (command_tx, command_rx) = std::sync::mpsc::sync_channel::<Command>(8);
    tokio::task::spawn_blocking(move || loop {
        if let Ok(mut client) = Client::connect(address, mac, Duration::from_secs(2)) {
            loop {
                let state = client.status().ok();
                let available = state.is_some();
                let off = state.as_ref().is_some_and(|s| s.mode == "off");
                if tx.send(state).is_err() {
                    return;
                }
                if !available {
                    break;
                }
                match command_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(command) => {
                        let result = match command {
                            Command::Temperature(value) => client.set_temperature(value),
                            Command::Power(value) => client.set_power(value),
                            Command::Mode(value) => {
                                if off {
                                    client.set_power(true).and_then(|_| client.set_mode(value))
                                } else {
                                    client.set_mode(value)
                                }
                            }
                            Command::Fan(value) => client.set_fan(value),
                            Command::Swing(value) => client.set_swing(value),
                        };
                        if result.is_err() {
                            let _ = tx.send(None);
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        } else if tx.send(None).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    });
    loop {
        tokio::select! {
            event=events.poll() => match event {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    mqtt.subscribe(format!("homecore/devices/{DEVICE}/cmd"),QoS::AtLeastOnce).await.map_err(|_| "MQTT subscribe failed")?;
                    mqtt.subscribe(format!("homecore/plugins/{PLUGIN}/manage/cmd"),QoS::AtLeastOnce).await.map_err(|_| "MQTT subscribe failed")?;
                    mqtt.publish(format!("homecore/plugins/{PLUGIN}/capabilities"),QoS::AtLeastOnce,true,json!({"spec":"1","plugin_id":PLUGIN,"actions":[],"ui":assets.as_ref().as_ref().map(|a|a.descriptor())}).to_string()).await.map_err(|_| "MQTT publish failed")?;
                    mqtt.publish(format!("homecore/plugins/{PLUGIN}/register"),QoS::AtLeastOnce,true,json!({"device_id":DEVICE,"plugin_id":PLUGIN,"name":"Living room AC","device_type":"thermostat"}).to_string()).await.map_err(|_| "MQTT publish failed")?;
                    mqtt.publish(format!("homecore/devices/{DEVICE}/schema"),QoS::AtLeastOnce,true,json!({"attributes":{"temperature":{"kind":"float","unit":"°C","writable":false},"setpoint":{"kind":"float","unit":"°C","writable":true,"min":17,"max":30,"step":1},"on":{"kind":"bool","writable":true},"hvac_mode":{"kind":"string","writable":true,"options":["off","heat","cool","auto","dry","fan_only"]},"fan_mode":{"kind":"string","writable":true,"options":["auto","low","medium","high","turbo","quiet"]},"swing":{"kind":"bool","writable":true},"vertical_vane":{"kind":"string","writable":true,"options":["off","swing"]}},"primary":["temperature","setpoint","hvac_mode"]}).to_string()).await.map_err(|_| "MQTT publish failed")?;
                    publish(&mqtt,rx.borrow_and_update().as_ref()).await?;
                }
                Ok(Event::Incoming(Incoming::Publish(message))) if !message.retain && rx.borrow().is_some() && message.topic == format!("homecore/devices/{DEVICE}/cmd") && message.payload.len() <= 1024 => {
                    if let Some(command) = command(&message.payload) { let _ = command_tx.try_send(command); }
                }
                Ok(Event::Incoming(Incoming::Publish(message))) if !message.retain && message.topic == format!("homecore/plugins/{PLUGIN}/manage/cmd") && message.payload.len() <= homecore_plugin_ui::MAX_REQUEST_BYTES => { let _ = ui_tx.try_send(message.payload.to_vec()); }
                Ok(_) => {},
                Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            },
            changed=rx.changed() => { changed.map_err(|_| "Device worker stopped")?; publish(&mqtt,rx.borrow_and_update().as_ref()).await?; }
        }
    }
}
async fn publish(
    mqtt: &AsyncClient,
    state: Option<&homecore_electrolux::ClimateStatus>,
) -> Result<(), &'static str> {
    if let Some(state) = state {
        mqtt.publish(format!("homecore/devices/{DEVICE}/state"),QoS::AtLeastOnce,true,json!({"temperature":state.current_temperature,"setpoint":state.target_temperature,"on":state.mode!="off","hvac_mode":state.mode,"fan_mode":state.fan,"swing":state.swing,"vertical_vane":if state.swing {"swing"} else {"off"}}).to_string()).await.map_err(|_| "MQTT publish failed")?;
    }
    mqtt.publish(
        AVAIL,
        QoS::AtLeastOnce,
        true,
        if state.is_some() { "online" } else { "offline" },
    )
    .await
    .map_err(|_| "MQTT publish failed")
}

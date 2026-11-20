use homecore_electrolux::transport::Client;
use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, QoS};
use serde_json::json;
use std::{env, fs::File, io::Read, net::SocketAddr, time::Duration};
use tokio::sync::watch;

const DEVICE: &str = "electrolux_living_room";
const PLUGIN: &str = "plugin.electrolux.living_room";
const AVAIL: &str = "homecore/devices/electrolux_living_room/availability";

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
    opts.set_last_will(LastWill::new(AVAIL, "offline", QoS::AtLeastOnce, true));
    let (mqtt, mut events) = AsyncClient::new(opts, 16);
    let (tx, mut rx) = watch::channel(None);
    tokio::task::spawn_blocking(move || loop {
        let state = Client::connect(address, mac, Duration::from_secs(2))
            .and_then(|mut client| client.status())
            .ok();
        if tx.send(state).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));
    });
    loop {
        tokio::select! {
            event=events.poll() => match event {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    mqtt.publish(format!("homecore/plugins/{PLUGIN}/register"),QoS::AtLeastOnce,true,json!({"device_id":DEVICE,"plugin_id":PLUGIN,"name":"Living room AC","device_type":"thermostat"}).to_string()).await.map_err(|_| "MQTT publish failed")?;
                    mqtt.publish(format!("homecore/devices/{DEVICE}/schema"),QoS::AtLeastOnce,true,json!({"attributes":{"temperature":{"kind":"float","unit":"°C","writable":false},"setpoint":{"kind":"float","unit":"°C","writable":false},"hvac_mode":{"kind":"string","writable":false},"fan_mode":{"kind":"string","writable":false},"swing":{"kind":"bool","writable":false}},"primary":["temperature","setpoint","hvac_mode"]}).to_string()).await.map_err(|_| "MQTT publish failed")?;
                    publish(&mqtt,rx.borrow_and_update().as_ref()).await?;
                }
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
        mqtt.publish(format!("homecore/devices/{DEVICE}/state"),QoS::AtLeastOnce,true,json!({"temperature":state.current_temperature,"setpoint":state.target_temperature,"hvac_mode":state.mode,"fan_mode":state.fan,"swing":state.swing}).to_string()).await.map_err(|_| "MQTT publish failed")?;
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

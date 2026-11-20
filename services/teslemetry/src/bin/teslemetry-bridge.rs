use rumqttc::{AsyncClient, LastWill, MqttOptions, QoS};
use serde_json::json;
use std::{env, fs::File, io::Read, time::Duration};
use tokio::{sync::watch, time::Instant};

const AVAILABILITY_TOPIC: &str = "homecore/devices/teslemetry_powerwall/availability";
const SCHEMA_TOPIC: &str = "homecore/devices/teslemetry_powerwall/schema";

struct PowerwallMqtt {
    client: AsyncClient,
    connections: watch::Receiver<()>,
}

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(env::var(name).map_err(|_| format!("Missing {name}"))?)
}
fn secret(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut raw = String::new();
    File::open(required(name)?)
        .map_err(|_| "Cannot read Powerwall secret")?
        .take(4097)
        .read_to_string(&mut raw)
        .map_err(|_| "Cannot read Powerwall secret")?;
    if raw.len() > 4096 {
        return Err(format!("Invalid {name}").into());
    }
    let value = raw.trim();
    if value.is_empty() || value.len() > 4096 || !value.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(format!("Invalid {name}").into());
    }
    Ok(value.to_string())
}

async fn register(mqtt: &AsyncClient, schema: &str) -> Result<(), rumqttc::ClientError> {
    mqtt.publish("homecore/plugins/plugin.teslemetry-powerwall/register", QoS::AtLeastOnce, true,
        json!({"device_id":"teslemetry_powerwall","plugin_id":"plugin.teslemetry-powerwall","name":"Tesla Powerwall","device_type":"sensor"}).to_string()).await?;
    mqtt.publish(SCHEMA_TOPIC, QoS::AtLeastOnce, true, schema)
        .await
}

fn mqtt_options(host: String, port: u16, username: &str, password: &str) -> MqttOptions {
    let mut options = MqttOptions::new("plugin.teslemetry-powerwall", host, port);
    options.set_credentials(username, password);
    options.set_keep_alive(Duration::from_secs(30));
    // UI replies carry at most 128 KiB as base64; bound unacknowledged copies.
    options.set_max_packet_size(8 * 1024, 256 * 1024);
    options.set_inflight(2);
    options.set_last_will(LastWill::new(
        AVAILABILITY_TOPIC,
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    options
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = required("TESLEMETRY_URL")?;
    let site: u64 = required("TESLEMETRY_SITE_ID")?.parse()?;
    let token = secret("TESLEMETRY_TOKEN_FILE")?;
    let interval: u64 = env::var("POLL_INTERVAL_SECS")
        .unwrap_or_else(|_| "60".into())
        .parse()?;
    if !(1..=3600).contains(&interval) {
        return Err("Invalid Powerwall interval".into());
    }
    let mqtt_host = required("MQTT_HOST")?;
    let mqtt_port: u16 = required("MQTT_PORT")?.parse()?;
    if mqtt_host != "127.0.0.1" || mqtt_port == 0 || site == 0 {
        return Err("Invalid Powerwall configuration".into());
    }
    let username = required("MQTT_USERNAME")?;
    let password = secret("MQTT_PASSWORD_FILE")?;
    let options = mqtt_options(mqtt_host.clone(), mqtt_port, &username, &password);
    let (mqtt, eventloop) = AsyncClient::new(options, 16);
    let (connected, connections) = watch::channel(());
    tokio::spawn(homecore_teslemetry::plugin::run(
        mqtt.clone(),
        eventloop,
        connected,
    ));
    let (totals_tx, totals_rx) = watch::channel(serde_json::Map::new());
    let (tariff_tx, tariff_rx) =
        watch::channel(homecore_teslemetry::totals::TariffState::default());
    tokio::spawn(homecore_teslemetry::totals::listen(
        base.clone(),
        site,
        token.clone(),
        totals_tx,
        tariff_tx,
    ));
    tokio::spawn(homecore_teslemetry::vehicle::supervise(
        base.clone(),
        token.clone(),
        homecore_teslemetry::vehicle::Broker {
            host: mqtt_host,
            port: mqtt_port,
            username,
            password,
        },
    ));
    poll_powerwall(
        PowerwallMqtt {
            client: mqtt,
            connections,
        },
        base,
        site,
        token,
        interval,
        totals_rx,
        tariff_rx,
    )
    .await
}

fn powerwall_schema(readings: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let mut attributes = serde_json::Map::new();
    for (name, value) in readings {
        let kind = if value.is_boolean() {
            "bool"
        } else if value.is_string() {
            "string"
        } else if value.is_i64() {
            "integer"
        } else {
            "float"
        };
        let mut attribute =
            json!({"kind":kind,"writable":false,"display_name":name.replace('_', " ")});
        if name.ends_with("_w") {
            attribute["unit"] = json!("W");
        }
        if name.ends_with("_wh") {
            attribute["unit"] = json!("Wh");
        }
        if name.ends_with("_pct") {
            attribute["unit"] = json!("%");
            attribute["min"] = json!(0);
            attribute["max"] = json!(100);
        }
        if matches!(
            name.as_str(),
            "tariff_buy_price_per_kwh" | "tariff_sell_price_per_kwh"
        ) {
            let currency = readings
                .get("tariff_currency")
                .and_then(serde_json::Value::as_str)
                .filter(|value| *value != "unspecified")
                .unwrap_or("currency");
            attribute["unit"] = json!(format!("{currency}/kWh"));
            attribute["display_name"] = json!(if name == "tariff_buy_price_per_kwh" {
                "Buy price"
            } else {
                "Feed-in price"
            });
        }
        attributes.insert(name.clone(), attribute);
    }
    json!({"attributes":attributes,"primary":["charge_pct","battery_power_w"]})
}

async fn publish_powerwall_readings(
    mqtt: &AsyncClient,
    readings: serde_json::Map<String, serde_json::Value>,
    schema: &mut String,
) -> Result<(), rumqttc::ClientError> {
    let next_schema = powerwall_schema(&readings).to_string();
    if next_schema != *schema {
        mqtt.publish(SCHEMA_TOPIC, QoS::AtLeastOnce, true, next_schema.as_str())
            .await?;
        *schema = next_schema;
    }
    mqtt.publish(
        "homecore/devices/teslemetry_powerwall/state",
        QoS::AtLeastOnce,
        true,
        serde_json::Value::Object(readings).to_string(),
    )
    .await?;
    mqtt.publish(AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "online")
        .await
}

async fn poll_powerwall(
    mqtt: PowerwallMqtt,
    base: String,
    site: u64,
    token: String,
    interval: u64,
    totals: watch::Receiver<serde_json::Map<String, serde_json::Value>>,
    tariffs: watch::Receiver<homecore_teslemetry::totals::TariffState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let PowerwallMqtt {
        client: mqtt,
        mut connections,
    } = mqtt;
    let mut schema = json!({"attributes":{"charge_pct":{"kind":"float","writable":false,"display_name":"Charge","unit":"%","min":0,"max":100},"battery_power_w":{"kind":"float","writable":false,"display_name":"Battery power","unit":"W"}},"primary":["charge_pct","battery_power_w"]}).to_string();
    connections.changed().await?;
    register(&mqtt, &schema).await?;
    let mut client = None;
    let mut next_poll = Instant::now();
    loop {
        if client.is_none() {
            client = homecore_teslemetry::http::client().ok();
        }
        let pause = tokio::time::sleep_until(next_poll);
        let request = async {
            pause.await;
            match client.as_ref() {
                Some(client) => {
                    homecore_teslemetry::teslemetry::fetch_energy_async(client, &base, site, &token)
                        .await
                }
                None => Err("HTTP client failed"),
            }
        };
        tokio::pin!(request);
        let reading = loop {
            tokio::select! {
                biased;
                changed = connections.changed() => {
                    changed?;
                    register(&mqtt, &schema).await?;
                }
                result = &mut request => break result,
            }
        };
        match reading {
            Ok((value, mut readings)) => {
                readings.extend(totals.borrow().clone());
                readings.extend(
                    tariffs
                        .borrow()
                        .readings(std::time::SystemTime::now().into()),
                );
                if !readings
                    .keys()
                    .any(|key| key.starts_with("wall_connector_"))
                {
                    readings.remove("wall_connectors_energy_wh");
                    readings.remove("wall_connectors_energy_date");
                }
                readings.insert("charge_pct".into(), value.charge_pct.into());
                readings.insert("battery_power_w".into(), value.power_w.into());
                publish_powerwall_readings(&mqtt, readings, &mut schema).await?;
            }
            Err(_) => {
                eprintln!("Powerwall reading unavailable");
                mqtt.publish(AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "offline")
                    .await?;
            }
        }
        next_poll = Instant::now() + Duration::from_secs(interval);
    }
}

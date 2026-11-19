use rumqttc::{AsyncClient, LastWill, MqttOptions, QoS};
use serde_json::json;
use std::{env, fs::File, io::Read, time::Duration};

const AVAILABILITY_TOPIC: &str = "homecore/devices/teslemetry_powerwall/availability";

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

async fn register(mqtt: &AsyncClient) -> Result<(), Box<dyn std::error::Error>> {
    mqtt.publish("homecore/plugins/plugin.teslemetry-powerwall/register", QoS::AtLeastOnce, true,
        json!({"device_id":"teslemetry_powerwall","plugin_id":"plugin.teslemetry-powerwall","name":"Tesla Powerwall","device_type":"sensor"}).to_string()).await?;
    mqtt.publish("homecore/devices/teslemetry_powerwall/schema", QoS::AtLeastOnce, true,
        json!({"attributes":{"charge_pct":{"kind":"float","writable":false,"display_name":"Charge","unit":"%","min":0,"max":100},"battery_power_w":{"kind":"float","writable":false,"display_name":"Battery power","unit":"W"}},"primary":["charge_pct","battery_power_w"]}).to_string()).await?;
    Ok(())
}

fn mqtt_options(host: String, port: u16) -> Result<MqttOptions, Box<dyn std::error::Error>> {
    let mut options = MqttOptions::new("plugin.teslemetry-powerwall", host, port);
    options.set_credentials(required("MQTT_USERNAME")?, secret("MQTT_PASSWORD_FILE")?);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_last_will(LastWill::new(
        AVAILABILITY_TOPIC,
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    Ok(options)
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
    let options = mqtt_options(mqtt_host, mqtt_port)?;
    let (mqtt, mut eventloop) = AsyncClient::new(options, 16);
    tokio::spawn(async move {
        loop {
            if eventloop.poll().await.is_err() {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });
    register(&mqtt).await?;
    poll_powerwall(mqtt, base, site, token, interval).await
}

async fn poll_powerwall(
    mqtt: AsyncClient,
    base: String,
    site: u64,
    token: String,
    interval: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let endpoint = base.clone();
        let bearer = token.clone();
        let reading = tokio::task::spawn_blocking(move || {
            let client =
                homecore_powerwall::http::blocking_client().map_err(|_| "HTTP client failed")?;
            homecore_powerwall::teslemetry::fetch_powerwall(&client, &endpoint, site, &bearer)
        })
        .await?;
        match reading {
            Ok(value) => {
                mqtt.publish(
                    "homecore/devices/teslemetry_powerwall/state",
                    QoS::AtLeastOnce,
                    true,
                    json!({"charge_pct":value.charge_pct,"battery_power_w":value.power_w})
                        .to_string(),
                )
                .await?;
                mqtt.publish(AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "online")
                    .await?;
            }
            Err(_) => {
                eprintln!("Powerwall reading unavailable");
                mqtt.publish(AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "offline")
                    .await?;
            }
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

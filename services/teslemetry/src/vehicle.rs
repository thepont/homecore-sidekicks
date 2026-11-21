//! Read-only, discovered Tesla vehicle telemetry. Vehicle data is never requested by waking a car.
use ring::digest::{digest, SHA256};
use rumqttc::{AsyncClient, LastWill, MqttOptions, QoS};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::task::JoinHandle;

struct AbortOnDrop(JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone)]
pub struct Broker {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

fn valid_vin(vin: &str) -> bool {
    vin.len() == 17
        && vin.bytes().all(|c| {
            c.is_ascii_digit() || (c.is_ascii_uppercase() && !matches!(c, b'I' | b'O' | b'Q'))
        })
}

fn device_id(vin: &str) -> String {
    let hash = digest(&SHA256, vin.as_bytes());
    format!(
        "teslemetry_vehicle_{}",
        hash.as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn snake(name: &str) -> Option<String> {
    if name.is_empty()
        || name.len() > 64
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return None;
    }
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 && !out.ends_with('_') {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    let sensitive = [
        "serial",
        "token",
        "secret",
        "password",
        "user_id",
        "vehicle_id",
        "access_key",
    ];
    if out == "vin"
        || out.ends_with("_vin")
        || out == "id"
        || out.ends_with("_id")
        || sensitive.iter().any(|x| out.contains(x))
    {
        None
    } else {
        Some(out)
    }
}

fn flatten(value: &Value, prefix: &str, depth: usize, out: &mut Map<String, Value>) {
    if depth > 3 || out.len() >= 256 {
        return;
    }
    if let Some(object) = value.as_object() {
        if object.len() > 128 {
            return;
        }
        for (key, value) in object {
            let Some(key) = snake(key) else {
                continue;
            };
            let name = if prefix.is_empty() {
                key
            } else {
                format!("{prefix}_{key}")
            };
            flatten(value, &name, depth + 1, out);
        }
    } else if !prefix.is_empty() && prefix.len() <= 128 {
        match value {
            Value::Null => {
                out.insert(prefix.into(), Value::Null);
            }
            Value::Bool(_) => {
                out.insert(prefix.into(), value.clone());
            }
            Value::Number(n) if n.as_f64().is_some_and(|n| n.is_finite()) => {
                out.insert(prefix.into(), value.clone());
            }
            Value::String(s)
                if !s.is_empty() && s.len() <= 128 && s.chars().all(|c| !c.is_control()) =>
            {
                out.insert(prefix.into(), value.clone());
            }
            _ => {}
        }
    }
}

pub fn parse_event(
    data: &str,
    vin: &str,
) -> Option<(chrono::DateTime<chrono::FixedOffset>, Map<String, Value>)> {
    if !valid_vin(vin) {
        return None;
    }
    let event: Value = serde_json::from_str(data).ok()?;
    if event.get("vin")?.as_str()? != vin {
        return None;
    }
    let observed = chrono::DateTime::parse_from_rfc3339(event.get("createdAt")?.as_str()?).ok()?;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    if observed > now + chrono::Duration::minutes(2) {
        return None;
    }
    let mut fields = Map::new();
    if let Some(data) = event.get("data") {
        flatten(data, "", 0, &mut fields);
    }
    if let Some(data) = event.get("vehicle_data") {
        flatten(data, "", 0, &mut fields);
    }
    if let Some(state) = event.get("state") {
        flatten(state, "vehicle_state", 0, &mut fields);
    }
    fields.retain(|key, value| {
        if value.is_null() {
            return true;
        }
        if value.as_str().is_some_and(|v| v.contains(vin)) {
            return false;
        }
        match key.as_str() {
            "battery_level" => value.as_f64().is_some_and(|v| (0.0..=100.0).contains(&v)),
            "location_latitude" => value.as_f64().is_some_and(|v| (-90.0..=90.0).contains(&v)),
            "location_longitude" => value
                .as_f64()
                .is_some_and(|v| (-180.0..=180.0).contains(&v)),
            _ => true,
        }
    });
    if fields.is_empty() {
        None
    } else {
        Some((observed, fields))
    }
}

async fn bounded_json(response: reqwest::Response) -> Option<Value> {
    let body = homecore_bounded_http::read_bounded(response, 65_536)
        .await
        .ok()?;
    serde_json::from_slice(&body).ok()
}

fn vehicle_display_name(vehicle: &Value, vin: &str) -> String {
    vehicle
        .get("display_name")
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty() && s.len() <= 64 && s.chars().all(|c| !c.is_control()) && !s.contains(vin)
        })
        .unwrap_or("Tesla vehicle")
        .to_owned()
}

pub async fn supervise(base: String, token: String, broker: Broker) {
    let Ok(url) = reqwest::Url::parse(&base) else {
        return;
    };
    if !crate::http::valid_base_url(&url) || broker.host != "127.0.0.1" || broker.port == 0 {
        return;
    }
    let Ok(client) = crate::http::client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
    else {
        return;
    };
    let mut running: HashMap<String, AbortOnDrop> = HashMap::new();
    loop {
        if let Ok(response) = client
            .get(format!("{}/api/1/vehicles", base.trim_end_matches('/')))
            .bearer_auth(&token)
            .send()
            .await
        {
            if let Ok(response) = response.error_for_status() {
                if let Some(value) = bounded_json(response).await {
                    if let Some(vehicles) = value
                        .get("response")
                        .and_then(Value::as_array)
                        .filter(|v| v.len() <= 16)
                    {
                        let mut seen = HashSet::new();
                        for vehicle in vehicles {
                            let Some(vin) = vehicle
                                .get("vin")
                                .and_then(Value::as_str)
                                .filter(|v| valid_vin(v))
                            else {
                                continue;
                            };
                            if !seen.insert(vin.to_owned()) {
                                continue;
                            }
                            if !running.contains_key(vin) {
                                let name = vehicle_display_name(vehicle, vin);
                                let task = tokio::spawn(follow(
                                    base.clone(),
                                    token.clone(),
                                    broker.clone(),
                                    vin.to_owned(),
                                    name,
                                ));
                                running.insert(vin.to_owned(), AbortOnDrop(task));
                            }
                        }
                        running.retain(|vin, task| seen.contains(vin) && !task.0.is_finished());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(900)).await;
    }
}

fn vehicle_schema(state: &Map<String, Value>) -> Value {
    let mut attrs = Map::new();
    for (key, value) in state {
        let kind = if value.is_boolean() {
            "bool"
        } else if value.is_string() {
            "string"
        } else if value.is_i64() || value.is_u64() {
            "integer"
        } else {
            "float"
        };
        let mut spec = json!({"kind":kind,"writable":false,"display_name":key.replace('_', " ")});
        if key == "battery_level" {
            spec["unit"] = json!("%");
        }
        if key.ends_with("_temp") {
            spec["unit"] = json!("°C");
        }
        attrs.insert(key.clone(), spec);
    }
    json!({"attributes":attrs,"primary":["battery_level", "vehicle_state"]})
}

fn apply_observation(
    state: &mut Map<String, Value>,
    times: &mut HashMap<String, chrono::DateTime<chrono::FixedOffset>>,
    key: String,
    value: Value,
    observed: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    if times
        .get(&key)
        .is_some_and(|previous| observed <= *previous)
    {
        return false;
    }
    times.insert(key.clone(), observed);
    if value.is_null() {
        state.remove(&key);
    } else {
        state.insert(key, value);
    }
    true
}

async fn publish_vehicle_state(
    mqtt: &AsyncClient,
    id: &str,
    availability: &str,
    state: &Map<String, Value>,
    latest: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    let schema = vehicle_schema(state);
    if mqtt
        .publish(
            format!("homecore/devices/{id}/schema"),
            QoS::AtLeastOnce,
            true,
            schema.to_string(),
        )
        .await
        .is_err()
    {
        return false;
    }
    if mqtt
        .publish(
            format!("homecore/devices/{id}/state"),
            QoS::AtLeastOnce,
            true,
            Value::Object(state.clone()).to_string(),
        )
        .await
        .is_err()
    {
        return false;
    }
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let fresh = now.signed_duration_since(latest).num_seconds() <= 300;
    let asleep = state
        .get("vehicle_state")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "online");
    mqtt.publish(
        availability,
        QoS::AtLeastOnce,
        true,
        if fresh && !asleep {
            "online"
        } else {
            "offline"
        },
    )
    .await
    .is_ok()
}

async fn open_vehicle_stream(
    client: &reqwest::Client,
    endpoint: String,
    token: &str,
) -> Option<reqwest::Response> {
    let response = client.get(endpoint).bearer_auth(token).send().await.ok()?;
    let response = response.error_for_status().ok()?;
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"))
        .then_some(response)
}

fn update_vehicle_event(
    data: &str,
    vin: &str,
    state: &mut Map<String, Value>,
    field_times: &mut HashMap<String, chrono::DateTime<chrono::FixedOffset>>,
    latest: &mut Option<chrono::DateTime<chrono::FixedOffset>>,
) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let (observed, fields) = parse_event(data, vin)?;
    let mut changed = false;
    for (key, value) in fields {
        if apply_observation(state, field_times, key, value, observed) {
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    let updated = latest.map_or(observed, |previous| previous.max(observed));
    *latest = Some(updated);
    Some(updated)
}

async fn follow(base: String, token: String, broker: Broker, vin: String, name: String) {
    let id = device_id(&vin);
    let availability = format!("homecore/devices/{id}/availability");
    let mut options = MqttOptions::new(
        format!("vehicle-{}", &id[id.len() - 16..]),
        broker.host,
        broker.port,
    );
    options.set_credentials(broker.username, broker.password);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_last_will(LastWill::new(
        &availability,
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (mqtt, mut events) = AsyncClient::new(options, 16);
    let _driver = AbortOnDrop(tokio::spawn(async move {
        loop {
            if events.poll().await.is_err() {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }));
    if mqtt.publish("homecore/plugins/plugin.teslemetry-powerwall/register", QoS::AtLeastOnce, false, json!({"device_id":id,"plugin_id":"plugin.teslemetry-powerwall","name":name,"device_type":"sensor"}).to_string()).await.is_err() { return; }
    if mqtt
        .publish(&availability, QoS::AtLeastOnce, true, "offline")
        .await
        .is_err()
    {
        return;
    }
    let Ok(client) = crate::http::client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(120))
        .build()
    else {
        return;
    };
    let mut latest = None;
    let mut state = Map::new();
    let mut field_times = HashMap::new();
    loop {
        let endpoint = format!(
            "{}/sse/{}?topics=state,data,vehicle_data",
            base.trim_end_matches('/'),
            vin
        );
        if let Some(mut response) = open_vehicle_stream(&client, endpoint, &token).await {
            let mut pending = Vec::new();
            let mut data = String::new();
            while let Ok(Some(chunk)) = response.chunk().await {
                pending.extend_from_slice(&chunk);
                if pending.len() > 65_536 {
                    break;
                }
                while let Some(end) = pending.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = pending.drain(..=end).collect();
                    let Ok(line) = std::str::from_utf8(&line[..end]) else {
                        continue;
                    };
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    if line.is_empty() {
                        if let Some(updated) = update_vehicle_event(
                            &data,
                            &vin,
                            &mut state,
                            &mut field_times,
                            &mut latest,
                        ) {
                            if !publish_vehicle_state(&mqtt, &id, &availability, &state, updated)
                                .await
                            {
                                return;
                            }
                        }
                        data.clear();
                    } else if let Some(line) = line.strip_prefix("data:") {
                        if data.len() + line.len() > 65_536 {
                            data.clear();
                            break;
                        }
                        if !data.is_empty() {
                            data.push('\n');
                        }
                        data.push_str(line.strip_prefix(' ').unwrap_or(line));
                    }
                }
            }
        }
        let _ = mqtt
            .publish(&availability, QoS::AtLeastOnce, true, "offline")
            .await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

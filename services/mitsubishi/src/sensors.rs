//! Publish observed BLE and Link sensors as bounded, read-only devices.
use crate::{bridge::Broker, config::Unit, source::Observation};
use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, QoS};
use serde_json::{json, Map, Value};
use std::time::Duration;
use tokio::{
    sync::watch,
    time::{self, MissedTickBehavior},
};

pub(crate) async fn run(
    unit: Unit,
    broker: Broker,
    slot: usize,
    mut latest: watch::Receiver<Option<Observation>>,
    interval: Duration,
    stale: Duration,
) {
    let is_link = slot == 4;
    let suffix = if is_link {
        "link".to_string()
    } else {
        format!("ble_{slot}")
    };
    let prefix = format!("{suffix}_");
    loop {
        let discovered = latest.borrow().as_ref().is_some_and(|v| {
            if is_link {
                v.state.values.get("link_sensor_present") == Some(&Value::Bool(true))
            } else {
                v.state.values.contains_key(&format!("{prefix}available"))
            }
        });
        if discovered {
            break;
        }
        if latest.changed().await.is_err() {
            return;
        }
    }
    let plugin = format!("plugin.mitsubishi.{}.{suffix}", unit.id);
    let device = format!("mitsubishi_{}_{suffix}", unit.id);
    let name = if is_link {
        format!("{} Link sensor", unit.name)
    } else {
        format!("{} sensor {}", unit.name, slot + 1)
    };
    let mut opts = MqttOptions::new(plugin.clone(), broker.host, broker.port);
    opts.set_credentials(broker.username, broker.password);
    opts.set_keep_alive(Duration::from_secs(15));
    opts.set_last_will(LastWill::new(
        format!("homecore/devices/{device}/availability"),
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (mqtt, mut events) = AsyncClient::new(opts, 32);
    let mut connected = false;
    let mut published = None;
    let mut tick = time::interval(interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            event = events.poll() => match event {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    connected = true;
                    published = None;
                    let registration = json!({"device_id":device,"name":name,"device_type":"temperature_sensor","parent_device_id":format!("mitsubishi_{}",unit.id)});
                    if send(&mqtt, format!("homecore/plugins/{plugin}/register"), registration.to_string()).await.is_err() { connected = false; }
                    if connected {
                        let observation = latest.borrow().clone();
                        publish(&mqtt, &device, &prefix, observation.as_ref(), stale, &mut published).await;
                    }
                }
                Ok(_) => {},
                Err(_) => { connected = false; time::sleep(Duration::from_secs(1)).await; }
            },
            changed = latest.changed() => {
                if changed.is_err() { return; }
                let observation = latest.borrow_and_update().clone();
                if connected { publish(&mqtt, &device, &prefix, observation.as_ref(), stale, &mut published).await; }
            }
            _ = tick.tick(), if connected => {
                let observation = latest.borrow().clone();
                publish(&mqtt, &device, &prefix, observation.as_ref(), stale, &mut published).await;
            }
        }
    }
}

async fn send(mqtt: &AsyncClient, topic: String, payload: String) -> Result<(), ()> {
    time::timeout(
        Duration::from_secs(2),
        mqtt.publish(topic, QoS::AtLeastOnce, true, payload),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

async fn publish(
    mqtt: &AsyncClient,
    device: &str,
    prefix: &str,
    observation: Option<&Observation>,
    stale: Duration,
    previous: &mut Option<(Value, Value, bool)>,
) {
    let observation = observation.filter(|v| {
        v.received.elapsed() < stale
            && v.state.values.get("cn105_connected") == Some(&Value::Bool(true))
    });
    let available = observation.is_some_and(|v| {
        if prefix == "link_" {
            v.state.values.get("link_sensor_present") == Some(&Value::Bool(true))
                && v.state.values.get("link_active") == Some(&Value::Bool(true))
                && v.state.values.get("link_stale") == Some(&Value::Bool(false))
        } else {
            v.state.values.get(&format!("{prefix}available")) == Some(&Value::Bool(true))
        }
    });
    let mut values = Map::new();
    let mut attributes = Map::new();
    if available {
        if let Some(v) = observation {
            for field in ["temperature", "humidity", "battery", "rssi", "type"] {
                let key = format!("{prefix}{field}");
                if let (Some(value), Some(attribute)) =
                    (v.state.values.get(&key), v.state.attributes.get(&key))
                {
                    values.insert(field.into(), value.clone());
                    attributes.insert(field.into(), attribute.clone());
                }
            }
        }
    }
    let primary = if attributes.contains_key("temperature") {
        vec!["temperature"]
    } else {
        vec![]
    };
    let base = format!("homecore/devices/{device}");
    let schema = json!({"attributes":attributes,"primary":primary});
    let state = Value::Object(values);
    let needs_schema = previous.as_ref().is_none_or(|p| p.0 != schema);
    let needs_state = previous.as_ref().is_none_or(|p| p.1 != state);
    let needs_availability = previous.as_ref().is_none_or(|p| p.2 != available);
    if (needs_schema
        && send(mqtt, format!("{base}/schema"), schema.to_string())
            .await
            .is_err())
        || (needs_state
            && send(mqtt, format!("{base}/state"), state.to_string())
                .await
                .is_err())
        || (needs_availability
            && send(
                mqtt,
                format!("{base}/availability"),
                if available { "online" } else { "offline" }.into(),
            )
            .await
            .is_err())
    {
        *previous = None;
    } else {
        *previous = Some((schema, state, available));
    }
}

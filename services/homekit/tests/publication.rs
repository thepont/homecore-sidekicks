#[path = "../../teslemetry/tests/support/mqtt_broker.rs"]
#[allow(dead_code)]
mod mqtt_broker;

use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};

#[tokio::test]
async fn climate_sensor_is_registered_with_temperature_humidity_and_battery() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    homecore_homekit::publish_sensors(
        &probe.client,
        "plugin.homekit",
        "02:11:22:33:44:55",
        &[(
            7,
            json!({"temperature":21.5,"humidity":42.0,"battery_pct":83}),
        )],
    )
    .await
    .unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(10), async {
        let mut received = HashMap::<String, Value>::new();
        while received.len() < 3 {
            let message = probe.messages.recv().await.unwrap();
            received.insert(
                message.topic,
                serde_json::from_slice(&message.payload).unwrap(),
            );
        }
        received
    })
    .await
    .unwrap();
    assert_eq!(
        messages["homecore/plugins/plugin.homekit/register"]["device_type"],
        "temperature_sensor"
    );
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_7/state"],
        json!({"temperature":21.5,"humidity":42.0,"battery_pct":83})
    );
    let schema = &messages["homecore/devices/homekit_021122334455_7/schema"]["attributes"];
    assert_eq!(schema["temperature"]["unit"], "°C");
    assert_eq!(schema["humidity"]["unit"], "%");
    assert_eq!(schema["battery_pct"]["unit"], "%");
}

#[tokio::test]
async fn contacts_are_registered_and_published_as_independent_homecore_devices() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    homecore_homekit::publish_contacts(
        &probe.client,
        "plugin.homekit",
        "02:11:22:33:44:55",
        &[
            (2, json!({"open":true,"battery_pct":69})),
            (3, json!({"open":false})),
        ],
    )
    .await
    .unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(10), async {
        let mut received = HashMap::<String, Vec<Value>>::new();
        while received.values().map(Vec::len).sum::<usize>() < 6 {
            let message = probe.messages.recv().await.unwrap();
            let value = serde_json::from_slice(&message.payload).unwrap();
            received.entry(message.topic).or_default().push(value);
        }
        received
    })
    .await
    .unwrap();
    let register = &messages["homecore/plugins/plugin.homekit/register"];
    assert_eq!(register.len(), 2);
    assert_eq!(register[0]["device_type"], "contact_sensor");
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_2/state"][0],
        json!({"open":true,"battery_pct":69})
    );
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_3/state"][0],
        json!({"open":false})
    );
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_2/schema"][0]["attributes"]["battery_pct"]
            ["unit"],
        "%"
    );
    assert!(
        messages["homecore/devices/homekit_021122334455_3/schema"][0]["attributes"]
            .get("battery_pct")
            .is_none()
    );
}

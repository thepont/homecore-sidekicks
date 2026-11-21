use homecore_test_support::mqtt_broker;

use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};

#[tokio::test]
async fn publication_registers_before_schema_and_keeps_current_state_for_new_subscribers() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    homecore_homekit::publish_sensors(
        &probe.client,
        "plugin.homekit",
        "02:11:22:33:44:55",
        &[(2, json!({"on":true}))],
    )
    .await
    .unwrap();

    let topics = tokio::time::timeout(Duration::from_secs(10), async {
        let mut topics = Vec::new();
        for _ in 0..3 {
            topics.push(probe.messages.recv().await.unwrap().topic);
        }
        topics
    })
    .await
    .unwrap();
    assert_eq!(
        topics,
        [
            "homecore/plugins/plugin.homekit/register",
            "homecore/devices/homekit_021122334455_2/schema",
            "homecore/devices/homekit_021122334455_2/state",
        ]
    );

    let mut late = mqtt_broker::Probe::start(broker.port).await;
    let retained = tokio::time::timeout(Duration::from_secs(10), async {
        let mut values = HashMap::new();
        for _ in 0..2 {
            let msg = late.messages.recv().await.unwrap();
            assert!(msg.retain);
            values.insert(
                msg.topic,
                serde_json::from_slice::<Value>(&msg.payload).unwrap(),
            );
        }
        values
    })
    .await
    .unwrap();
    assert!(retained.contains_key("homecore/devices/homekit_021122334455_2/schema"));
    assert_eq!(
        retained["homecore/devices/homekit_021122334455_2/state"],
        json!({"on":true})
    );
}

#[tokio::test]
async fn unavailable_sensor_is_registered_with_its_type_without_inventing_a_reading() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    homecore_homekit::publish_sensors(
        &probe.client,
        "plugin.homekit",
        "02:11:22:33:44:55",
        &[(5, json!({"_offline":true,"_type":"temperature_sensor"}))],
    )
    .await
    .unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(10), async {
        let mut received = HashMap::<String, Value>::new();
        while received.len() < 2 {
            let msg = probe.messages.recv().await.unwrap();
            received.insert(msg.topic, serde_json::from_slice(&msg.payload).unwrap());
        }
        received
    })
    .await
    .unwrap();
    assert_eq!(
        messages["homecore/plugins/plugin.homekit/register"]["device_type"],
        "temperature_sensor"
    );
    assert!(messages.contains_key("homecore/devices/homekit_021122334455_5/schema"));
    assert!(!messages.contains_key("homecore/devices/homekit_021122334455_5/state"));
}

#[test]
fn only_well_formed_commands_for_the_expected_bridge_are_accepted() {
    let topic = "homecore/devices/homekit_021122334455_7/cmd";
    assert_eq!(
        homecore_homekit::outlet_command("02:11:22:33:44:55", topic, &json!({"on":true})),
        Some((7, true))
    );
    assert_eq!(
        homecore_homekit::outlet_command("02:11:22:33:44:55", topic, &json!({"on":"true"})),
        None
    );
    assert_eq!(
        homecore_homekit::outlet_command("02:11:22:33:44:55", topic, &json!({"on":true,"extra":1})),
        None
    );
    assert_eq!(
        homecore_homekit::outlet_command(
            "02:11:22:33:44:55",
            "homecore/devices/homekit_aaaaaaaaaaaa_7/cmd",
            &json!({"on":true})
        ),
        None
    );
    assert_eq!(
        homecore_homekit::outlet_command(
            "02:11:22:33:44:55",
            "homecore/devices/homekit_021122334455_07/cmd",
            &json!({"on":true})
        ),
        None
    );
}

#[tokio::test]
async fn outlet_is_published_as_a_switch_with_its_on_and_in_use_states() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    homecore_homekit::publish_sensors(
        &probe.client,
        "plugin.homekit",
        "02:11:22:33:44:55",
        &[(2, json!({"on":true,"in_use":false}))],
    )
    .await
    .unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(10), async {
        let mut received = HashMap::<String, Value>::new();
        while received.len() < 3 {
            let msg = probe.messages.recv().await.unwrap();
            received.insert(msg.topic, serde_json::from_slice(&msg.payload).unwrap());
        }
        received
    })
    .await
    .unwrap();
    assert_eq!(
        messages["homecore/plugins/plugin.homekit/register"]["device_type"],
        "switch"
    );
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_2/state"],
        json!({"on":true,"in_use":false})
    );
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_2/schema"]["attributes"]["on"]["writable"],
        true
    );
}

#[tokio::test]
async fn smoke_detector_reports_alarm_fault_and_battery_as_a_smoke_detector() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    homecore_homekit::publish_sensors(
        &probe.client,
        "plugin.homekit",
        "02:11:22:33:44:55",
        &[(9, json!({"smoke":true,"fault":false,"battery_pct":74}))],
    )
    .await
    .unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(10), async {
        let mut received = HashMap::<String, Value>::new();
        while received.len() < 3 {
            let msg = probe.messages.recv().await.unwrap();
            received.insert(msg.topic, serde_json::from_slice(&msg.payload).unwrap());
        }
        received
    })
    .await
    .unwrap();
    assert_eq!(
        messages["homecore/plugins/plugin.homekit/register"]["device_type"],
        "smoke_sensor"
    );
    assert_eq!(
        messages["homecore/devices/homekit_021122334455_9/state"],
        json!({"smoke":true,"fault":false,"battery_pct":74})
    );
    let attributes = &messages["homecore/devices/homekit_021122334455_9/schema"]["attributes"];
    assert_eq!(attributes["smoke"]["kind"], "bool");
    assert_eq!(attributes["fault"]["kind"], "bool");
}

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

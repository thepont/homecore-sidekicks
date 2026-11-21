use homecore_test_support::homecore;

use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde_json::{json, Value};
use std::time::Duration;

async fn device(core: &homecore::Core, expected_type: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(response) = core.get("/devices/mitsubishi_room_a").send().await {
                if response.status().is_success() {
                    let device: Value = response.json().await.unwrap();
                    if device["device_type"] == expected_type {
                        return device;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("device type must update")
}

#[tokio::test]
#[ignore = "requires Docker and pinned homeCore image"]
async fn reregistration_updates_type_without_losing_identity_or_area() {
    let core = homecore::Core::start_for_plugin("plugin.mitsubishi.room_a").await;
    let mut options = MqttOptions::new("registration-contract", "127.0.0.1", core.mqtt_port);
    options.set_credentials(
        "plugin.mitsubishi.room_a",
        std::fs::read_to_string(core.root.join("mqtt-password")).unwrap(),
    );
    let (client, mut events) = AsyncClient::new(options, 10);
    let event_loop = tokio::spawn(async move {
        loop {
            if events.poll().await.is_err() {
                break;
            }
        }
    });
    let topic = "homecore/plugins/plugin.mitsubishi.room_a/register";
    let registration = |device_type| {
        json!({"device_id":"mitsubishi_room_a", "name":"Room A AC", "area":"bedroom", "device_type":device_type}).to_string()
    };
    client
        .publish(topic, QoS::AtLeastOnce, true, registration("sensor"))
        .await
        .unwrap();
    let before = device(&core, "sensor").await;
    client.publish(topic, QoS::AtLeastOnce, true, json!({"device_id":"mitsubishi_room_a", "name":"Room A AC", "device_type":"thermostat"}).to_string()).await.unwrap();
    let after = device(&core, "thermostat").await;
    for field in ["device_id", "plugin_id", "name", "area", "canonical_name"] {
        assert_eq!(
            before[field], after[field],
            "{field} changed during migration"
        );
    }
    assert_eq!(after["area"], "bedroom");
    event_loop.abort();
}

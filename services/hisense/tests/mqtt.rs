use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use homecore_hisense::{run, Broker, Hisense};
use homecore_test_support::mqtt_broker;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    future::IntoFuture,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::net::TcpListener;

#[tokio::test]
#[ignore = "requires local Docker and the pinned Mosquitto image"]
async fn publishes_read_only_media_state_and_marks_unreachable_offline() {
    let mut broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let mut heartbeat_probe = mqtt_broker::Probe::start(broker.port).await;
    let active = Arc::new(AtomicBool::new(true));
    async fn response(
        State(active): State<Arc<AtomicBool>>,
        headers: HeaderMap,
    ) -> (StatusCode, &'static str) {
        if !active.load(Ordering::SeqCst) {
            return (StatusCode::SERVICE_UNAVAILABLE, "");
        }
        if headers
            .get("soapaction")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("GetVolume")
        {
            (StatusCode::OK, "<r><CurrentVolume>14</CurrentVolume></r>")
        } else {
            (StatusCode::OK, "<r><CurrentMute>0</CurrentMute></r>")
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(
        axum::serve(
            listener,
            Router::new()
                .route("/upnp/control/mingusrcr", post(response))
                .with_state(active.clone()),
        )
        .into_future(),
    );
    let tv = Hisense::new(addr, Duration::from_secs(1)).unwrap();
    let task = tokio::spawn(run(
        tv,
        Broker {
            host: "127.0.0.1".into(),
            port: broker.port,
            username: "fixture-user".into(),
            password: "fixture-password".into(),
        },
        Duration::from_millis(100),
    ));
    async fn wait(probe: &mut mqtt_broker::Probe, topic: &str, expected: &str) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let p = probe.messages.recv().await.unwrap();
                if p.topic == topic && String::from_utf8_lossy(&p.payload).contains(expected) {
                    break;
                }
            }
        })
        .await
        .expect("expected MQTT publication");
    }
    wait(
        &mut probe,
        "homecore/plugins/plugin.hisense/register",
        "media_player",
    )
    .await;
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/state",
        "\"volume\":14",
    )
    .await;
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/availability",
        "online",
    )
    .await;
    let heartbeat = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let message = heartbeat_probe.messages.recv().await.unwrap();
            if message.topic == "homecore/plugins/plugin.hisense/heartbeat" {
                break message;
            }
        }
    })
    .await
    .expect("plugin heartbeat");
    assert_eq!(heartbeat.qos, rumqttc::QoS::AtLeastOnce);
    let heartbeat: Value = serde_json::from_slice(&heartbeat.payload).unwrap();
    assert_eq!(heartbeat["protocol_version"], "0.1.5");
    assert_eq!(heartbeat["version"], env!("CARGO_PKG_VERSION"));
    assert!(heartbeat["uptime_secs"].is_u64());

    active.store(false, Ordering::SeqCst);
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/availability",
        "offline",
    )
    .await;
    active.store(true, Ordering::SeqCst);
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/state",
        "\"volume\":14",
    )
    .await;
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/availability",
        "online",
    )
    .await;
    broker.restart().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    wait(
        &mut probe,
        "homecore/plugins/plugin.hisense/register",
        "media_player",
    )
    .await;
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/state",
        "\"volume\":14",
    )
    .await;
    wait(
        &mut probe,
        "homecore/devices/hisense_living_room/availability",
        "online",
    )
    .await;
    let mut late_subscriber = mqtt_broker::Probe::start(broker.port).await;
    let mut announcements = BTreeMap::from([
        (
            "homecore/plugins/plugin.hisense/register",
            json!({
                "device_id": "hisense_living_room", "plugin_id": "plugin.hisense",
                "name": "Living room TV", "device_type": "media_player"
            }),
        ),
        (
            "homecore/plugins/plugin.hisense/capabilities",
            json!({"spec": "1", "plugin_id": "plugin.hisense", "actions": []}),
        ),
        (
            "homecore/devices/hisense_living_room/schema",
            json!({
                "attributes": {
                    "volume": {"kind": "integer", "writable": false, "min": 0, "max": 100, "unit": "%"},
                    "muted": {"kind": "bool", "writable": false},
                },
                "primary": ["volume", "muted"],
            }),
        ),
        (
            "homecore/devices/hisense_living_room/state",
            json!({"volume": 14, "muted": false}),
        ),
    ]);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !announcements.is_empty() {
            let message = late_subscriber.messages.recv().await.unwrap();
            if let Some(expected) = announcements.remove(message.topic.as_str()) {
                assert!(
                    message.retain,
                    "this message must be retained: {}",
                    message.topic
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&message.payload).unwrap(),
                    expected
                );
            }
        }
    })
    .await
    .expect("retained HomeCore announcements for a late subscriber");

    task.abort();
    wait(
        &mut late_subscriber,
        "homecore/devices/hisense_living_room/availability",
        "offline",
    )
    .await;
    server.abort();
}

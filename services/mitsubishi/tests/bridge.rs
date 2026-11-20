#[allow(dead_code)]
#[path = "../../teslemetry/tests/support/mqtt_broker.rs"]
mod mqtt_broker;

use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use rumqttc::Publish;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::Message;

struct Source {
    url: String,
    connections: mpsc::UnboundedReceiver<Session>,
    snapshot: watch::Sender<Option<Value>>,
    task: JoinHandle<()>,
}

struct Session {
    commands: mpsc::Sender<Message>,
    received: mpsc::UnboundedReceiver<Message>,
}

impl Drop for Source {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Source {
    async fn start(initial: Value) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        let (connected, connections) = mpsc::unbounded_channel();
        let (snapshot, snapshots) = watch::channel(Some(initial));
        let task = tokio::spawn(async move {
            let mut sessions = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        let connected = connected.clone();
                        let mut snapshots = snapshots.clone();
                        sessions.spawn(async move {
                            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                            let (commands, mut pending) = mpsc::channel(8);
                            let (observed, received) = mpsc::unbounded_channel();
                            connected.send(Session { commands, received }).unwrap();
                            let mut heartbeat = tokio::time::interval(Duration::from_millis(500));
                            loop {
                                tokio::select! {
                                    _ = heartbeat.tick() => {
                                        let current = snapshots.borrow_and_update().clone();
                                        if let Some(current) = current {
                                            if ws.send(Message::Text(current.to_string().into())).await.is_err() { return; }
                                        }
                                    }
                                    command = pending.recv() => {
                                        let Some(command) = command else { return; };
                                        if ws.send(command).await.is_err() { return; }
                                    }
                                    message = ws.next() => match message {
                                        Some(Ok(message)) => {
                                            let closed = message.is_close();
                                            let _ = observed.send(message);
                                            if closed || ws.flush().await.is_err() { return; }
                                        }
                                        _ => return,
                                    }
                                }
                            }
                        });
                    }
                    _ = sessions.join_next(), if !sessions.is_empty() => {}
                }
            }
        });
        Self {
            url,
            connections,
            snapshot,
            task,
        }
    }

    async fn connected(&mut self) -> Session {
        tokio::time::timeout(Duration::from_secs(5), self.connections.recv())
            .await
            .expect("controller must connect")
            .unwrap()
    }
}

impl Session {
    async fn assert_read_only(&mut self) {
        self.commands
            .send(Message::Ping(b"read-only-barrier".to_vec().into()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match self
                    .received
                    .recv()
                    .await
                    .expect("controller session stays open")
                {
                    Message::Text(_) | Message::Binary(_) => {
                        panic!("telemetry must not send application frames to the controller")
                    }
                    Message::Pong(bytes) if bytes.as_ref() == b"read-only-barrier" => return,
                    _ => {}
                }
            }
        })
        .await
        .expect("controller must answer the read-only ping barrier");
    }
}

struct Adapter {
    child: Child,
    root: PathBuf,
}
impl Drop for Adapter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for name in ["config.json", "mqtt-password"] {
            let _ = std::fs::remove_file(self.root.join(name));
        }
        let _ = std::fs::remove_dir(&self.root);
    }
}

fn start_adapter(port: u16, first: &Source, second: &Source) -> Adapter {
    start_adapter_with_staleness(port, first, second, 2)
}

fn start_adapter_with_staleness(port: u16, first: &Source, second: &Source, stale: u64) -> Adapter {
    let root =
        std::env::temp_dir().join(format!("mitsubishi-contract-{}-{port}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("mqtt-password"), "fixture-password").unwrap();
    std::fs::write(
        root.join("config.json"),
        json!({
            "units": [
                {"id":"room_a", "name":"Room A", "url":first.url},
                {"id":"room_b", "name":"Room B", "url":second.url}
        ], "publish_interval_secs":1, "stale_after_secs":stale
        })
        .to_string(),
    )
    .unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_mitsubishi-bridge"))
        .env("MITSUBISHI_CONFIG_FILE", root.join("config.json"))
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env(
            "HC_PLUGIN_UI_DIR",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/ac-card"),
        )
        .env("MQTT_PASSWORD_FILE", root.join("mqtt-password"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Adapter { child, root }
}

fn snapshot(room: f64, target: f64, mode: &str) -> Value {
    json!({"type":"state", "connected":true, "power":true,
        "room":room, "target":target, "mode":mode, "fan":"2", "operating":true})
}

async fn until(
    probe: &mut mqtt_broker::Probe,
    topic: &str,
    predicate: impl Fn(&[u8]) -> bool,
) -> Publish {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let message = probe
                .messages
                .recv()
                .await
                .expect("MQTT probe stays connected");
            if message.topic == topic && predicate(&message.payload) {
                return message;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected MQTT publication on {topic}"))
}

async fn state(probe: &mut mqtt_broker::Probe, id: &str, temperature: f64) -> Value {
    let message = until(
        probe,
        &format!("homecore/devices/mitsubishi_{id}/state"),
        |bytes| {
            serde_json::from_slice::<Value>(bytes)
                .is_ok_and(|value| value["temperature"] == temperature)
        },
    )
    .await;
    serde_json::from_slice(&message.payload).unwrap()
}

async fn retained(port: u16, topic: &str) -> Publish {
    let mut probe = mqtt_broker::Probe::start(port).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let message = probe.messages.recv().await.unwrap();
            if message.topic == topic && message.retain {
                return message;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected retained MQTT publication on {topic}"))
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn two_controllers_publish_independent_read_only_climate_devices() {
    let broker = mqtt_broker::Broker::start().await;
    let mut first = Source::start(snapshot(25.2, 23.5, "cool")).await;
    let mut second = Source::start(snapshot(18.1, 21.0, "heat")).await;
    let _adapter = start_adapter(broker.port, &first, &second);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let a = state(&mut probe, "room_a", 25.2).await;
    assert_eq!(a["setpoint"], 23.5);
    assert_eq!(a["hvac_mode"], "cool");
    let mut replay = mqtt_broker::Probe::start(broker.port).await;
    let b = state(&mut replay, "room_b", 18.1).await;
    assert_eq!(b["setpoint"], 21.0);
    assert_eq!(b["hvac_mode"], "heat");
    for (id, name) in [("room_a", "Room A"), ("room_b", "Room B")] {
        let registration = retained(
            broker.port,
            &format!("homecore/plugins/plugin.mitsubishi.{id}/register"),
        )
        .await;
        let registration: Value = serde_json::from_slice(&registration.payload).unwrap();
        assert_eq!(registration["device_id"], format!("mitsubishi_{id}"));
        assert_eq!(registration["plugin_id"], format!("plugin.mitsubishi.{id}"));
        assert_eq!(registration["name"], name);
        assert_eq!(registration["device_type"], "sensor");
        let schema = retained(
            broker.port,
            &format!("homecore/devices/mitsubishi_{id}/schema"),
        )
        .await;
        let schema: Value = serde_json::from_slice(&schema.payload).unwrap();
        assert_eq!(schema["attributes"]["temperature"]["unit"], "°C");
        assert_eq!(schema["attributes"]["setpoint"]["unit"], "°C");
        for attribute in schema["attributes"].as_object().unwrap().values() {
            assert_eq!(attribute["writable"], false);
        }
    }
    first
        .snapshot
        .send_replace(Some(snapshot(26.3, 24.0, "cool")));
    assert_eq!(state(&mut probe, "room_a", 26.3).await["setpoint"], 24.0);
    first.connected().await.assert_read_only().await;
    second.connected().await.assert_read_only().await;
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn quiet_or_disconnected_controller_withdraws_only_its_own_readings() {
    let broker = mqtt_broker::Broker::start().await;
    let mut first = Source::start(snapshot(24.2, 22.5, "cool")).await;
    let mut second = Source::start(snapshot(18.3, 20.5, "heat")).await;
    let _adapter = start_adapter(broker.port, &first, &second);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    state(&mut probe, "room_a", 24.2).await;
    let mut first_session = first.connected().await;
    let mut second_session = second.connected().await;
    first_session.assert_read_only().await;
    second_session.assert_read_only().await;

    let mut reduced = snapshot(24.8, 22.0, "cool");
    reduced.as_object_mut().unwrap().remove("fan");
    reduced["target"] = Value::Null;
    first.snapshot.send_replace(Some(reduced));
    let reduced = state(&mut probe, "room_a", 24.8).await;
    assert!(!reduced.as_object().unwrap().contains_key("fan_mode"));
    assert!(!reduced.as_object().unwrap().contains_key("setpoint"));
    let schema = retained(broker.port, "homecore/devices/mitsubishi_room_a/schema").await;
    let schema: Value = serde_json::from_slice(&schema.payload).unwrap();
    assert!(schema["attributes"].get("fan_mode").is_none());
    assert!(schema["attributes"].get("setpoint").is_none());
    let stored = retained(broker.port, "homecore/devices/mitsubishi_room_a/state").await;
    assert_eq!(
        serde_json::from_slice::<Value>(&stored.payload).unwrap(),
        reduced
    );

    // A live TCP/WebSocket connection cannot keep an old observation current.
    first.snapshot.send_replace(None);
    until(
        &mut probe,
        "homecore/devices/mitsubishi_room_a/availability",
        |bytes| bytes == b"offline",
    )
    .await;
    let stored = retained(broker.port, "homecore/devices/mitsubishi_room_a/state").await;
    assert_eq!(
        serde_json::from_slice::<Value>(&stored.payload).unwrap(),
        json!({})
    );
    second
        .snapshot
        .send_replace(Some(snapshot(19.4, 21.5, "heat")));
    state(&mut probe, "room_b", 19.4).await;
    let availability = retained(
        broker.port,
        "homecore/devices/mitsubishi_room_b/availability",
    )
    .await;
    assert_eq!(availability.payload.as_ref(), b"online");

    first
        .snapshot
        .send_replace(Some(snapshot(25.6, 23.0, "cool")));
    state(&mut probe, "room_a", 25.6).await;
    let mut recovered = first.connected().await;
    recovered.assert_read_only().await;
    first.snapshot.send_replace(None);
    recovered.commands.send(Message::Close(None)).await.unwrap();
    until(
        &mut probe,
        "homecore/devices/mitsubishi_room_a/availability",
        |bytes| bytes == b"offline",
    )
    .await;
    let stored = retained(broker.port, "homecore/devices/mitsubishi_room_a/state").await;
    assert_eq!(
        serde_json::from_slice::<Value>(&stored.payload).unwrap(),
        json!({})
    );
    second
        .snapshot
        .send_replace(Some(snapshot(19.7, 22.0, "heat")));
    state(&mut probe, "room_b", 19.7).await;
    second_session.assert_read_only().await;

    first
        .snapshot
        .send_replace(Some(snapshot(26.1, 24.5, "cool")));
    state(&mut probe, "room_a", 26.1).await;
    first.connected().await.assert_read_only().await;
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn broker_restart_recovers_both_devices_and_process_death_marks_both_offline() {
    let mut broker = mqtt_broker::Broker::start().await;
    let mut first = Source::start(snapshot(25.9, 22.0, "cool")).await;
    let mut second = Source::start(snapshot(17.6, 21.5, "heat")).await;
    let mut adapter = start_adapter(broker.port, &first, &second);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    state(&mut probe, "room_a", 25.9).await;
    let mut first_session = first.connected().await;
    let mut second_session = second.connected().await;
    first_session.assert_read_only().await;
    second_session.assert_read_only().await;
    drop(probe);

    // Persistence is disabled in this owned broker. Recovery must republish discovery.
    broker.restart().await;
    first
        .snapshot
        .send_replace(Some(snapshot(26.4, 23.0, "cool")));
    second
        .snapshot
        .send_replace(Some(snapshot(18.7, 22.0, "heat")));
    // The original source sessions keep running while their MQTT connections recover.
    first_session.assert_read_only().await;
    second_session.assert_read_only().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    state(&mut probe, "room_a", 26.4).await;
    let mut replay = mqtt_broker::Probe::start(broker.port).await;
    state(&mut replay, "room_b", 18.7).await;
    for id in ["room_a", "room_b"] {
        let registration = retained(
            broker.port,
            &format!("homecore/plugins/plugin.mitsubishi.{id}/register"),
        )
        .await;
        let registration: Value = serde_json::from_slice(&registration.payload).unwrap();
        assert_eq!(registration["device_id"], format!("mitsubishi_{id}"));
        let schema = retained(
            broker.port,
            &format!("homecore/devices/mitsubishi_{id}/schema"),
        )
        .await;
        let schema: Value = serde_json::from_slice(&schema.payload).unwrap();
        assert_eq!(schema["attributes"]["temperature"]["writable"], false);
        let availability = retained(
            broker.port,
            &format!("homecore/devices/mitsubishi_{id}/availability"),
        )
        .await;
        assert_eq!(availability.payload.as_ref(), b"online");
    }

    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    adapter.child.kill().unwrap();
    adapter.child.wait().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut first_offline = false;
        let mut second_offline = false;
        while !(first_offline && second_offline) {
            let message = probe.messages.recv().await.unwrap();
            if message.payload.as_ref() == b"offline" {
                first_offline |= message.topic == "homecore/devices/mitsubishi_room_a/availability";
                second_offline |=
                    message.topic == "homecore/devices/mitsubishi_room_b/availability";
            }
        }
    })
    .await
    .expect("each MQTT last will must mark its own unit offline after process death");
    for id in ["room_a", "room_b"] {
        let availability = retained(
            broker.port,
            &format!("homecore/devices/mitsubishi_{id}/availability"),
        )
        .await;
        assert_eq!(availability.payload.as_ref(), b"offline");
    }
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn malformed_and_oversized_controller_frames_withdraw_before_the_reading_expires() {
    let broker = mqtt_broker::Broker::start().await;
    let mut first = Source::start(snapshot(27.3, 24.0, "cool")).await;
    let second = Source::start(snapshot(18.6, 22.0, "heat")).await;
    // Eight seconds separates rejection from ordinary expiry; assertions allow five.
    let _adapter = start_adapter_with_staleness(broker.port, &first, &second, 8);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    state(&mut probe, "room_a", 27.3).await;
    let mut session = first.connected().await;
    let malformed = json!({"type":"state", "connected":true, "room":"invalid-number"});
    let oversized =
        json!({"type":"state", "connected":true, "room":29.9, "padding":"x".repeat(32768)});
    for (bad, recovery) in [(malformed, 27.6), (oversized, 28.2)] {
        first.snapshot.send_replace(None);
        // A fresh state followed by its protocol pong establishes the pre-fault deadline.
        session
            .commands
            .send(Message::Text(
                snapshot(27.3, 24.0, "cool").to_string().into(),
            ))
            .await
            .unwrap();
        session.assert_read_only().await;
        session
            .commands
            .send(Message::Text(bad.to_string().into()))
            .await
            .unwrap();
        until(
            &mut probe,
            "homecore/devices/mitsubishi_room_a/availability",
            |bytes| bytes == b"offline",
        )
        .await;
        let stored = retained(broker.port, "homecore/devices/mitsubishi_room_a/state").await;
        assert_eq!(
            serde_json::from_slice::<Value>(&stored.payload).unwrap(),
            json!({})
        );
        first
            .snapshot
            .send_replace(Some(snapshot(recovery, 24.5, "cool")));
        state(&mut probe, "room_a", recovery).await;
        session = first.connected().await;
        session.assert_read_only().await;
    }
}

async fn metadata_until_reconnect(source: &mut Source, session: &Session) -> Session {
    let connected = source.connected();
    tokio::pin!(connected);
    let mut traffic = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            next = &mut connected => return next,
            _ = traffic.tick() => {
                for value in [json!({"type":"deviceInfo", "fw":"v0.2.8"}), json!({"type":"log", "msg":"controller diagnostic"})] {
                    let _ = session.commands.send(Message::Text(value.to_string().into())).await;
                }
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn metadata_and_logs_do_not_keep_readings_fresh_or_cross_connection_lifetimes() {
    let broker = mqtt_broker::Broker::start().await;
    let mut first = Source::start(snapshot(23.7, 21.0, "cool")).await;
    let second = Source::start(snapshot(17.9, 20.0, "heat")).await;
    let _adapter = start_adapter(broker.port, &first, &second);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    state(&mut probe, "room_a", 23.7).await;
    let mut session = first.connected().await;
    session
        .commands
        .send(Message::Text(
            json!({"type":"deviceInfo", "fw":"v0.2.8"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    until(
        &mut probe,
        "homecore/devices/mitsubishi_room_a/state",
        |bytes| {
            serde_json::from_slice::<Value>(bytes).is_ok_and(|value| value["firmware"] == "v0.2.8")
        },
    )
    .await;
    session.assert_read_only().await;
    first.snapshot.send_replace(None);

    // Existing readings expire even while metadata and logs arrive continuously.
    let mut session = metadata_until_reconnect(&mut first, &session).await;
    let stored = retained(broker.port, "homecore/devices/mitsubishi_room_a/state").await;
    assert_eq!(
        serde_json::from_slice::<Value>(&stored.payload).unwrap(),
        json!({})
    );
    let availability = retained(
        broker.port,
        "homecore/devices/mitsubishi_room_a/availability",
    )
    .await;
    assert_eq!(availability.payload.as_ref(), b"offline");
    session.assert_read_only().await;

    // This connection has never sent state. Metadata cannot make it ready or immortal.
    let mut session = metadata_until_reconnect(&mut first, &session).await;
    let availability = retained(
        broker.port,
        "homecore/devices/mitsubishi_room_a/availability",
    )
    .await;
    assert_eq!(availability.payload.as_ref(), b"offline");
    first
        .snapshot
        .send_replace(Some(snapshot(24.6, 22.0, "cool")));
    let recovered = state(&mut probe, "room_a", 24.6).await;
    assert!(recovered.get("firmware").is_none());
    let schema = retained(broker.port, "homecore/devices/mitsubishi_room_a/schema").await;
    let schema: Value = serde_json::from_slice(&schema.payload).unwrap();
    assert!(schema["attributes"].get("firmware").is_none());
    session.assert_read_only().await;
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn both_units_advertise_and_serve_the_same_ac_card_without_controller_commands() {
    let broker = mqtt_broker::Broker::start().await;
    let mut first = Source::start(snapshot(24.2, 22.5, "cool")).await;
    let mut second = Source::start(snapshot(18.3, 20.5, "heat")).await;
    let _adapter = start_adapter(broker.port, &first, &second);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    state(&mut probe, "room_a", 24.2).await;
    state(&mut probe, "room_b", 18.3).await;
    let mut builds = Vec::new();
    for (id, request_id) in [
        ("room_a", "123e4567-e89b-42d3-a456-426614174001"),
        ("room_b", "123e4567-e89b-42d3-a456-426614174002"),
    ] {
        let prefix = format!("homecore/plugins/plugin.mitsubishi.{id}");
        let manifest = retained(broker.port, &format!("{prefix}/capabilities")).await;
        let manifest: Value = serde_json::from_slice(&manifest.payload).unwrap();
        let ui = &manifest["ui"];
        assert_eq!(ui["host_api"], 1);
        assert_eq!(ui["slots"][0]["element"], "hc-ac-card");
        assert_eq!(ui["slots"][0]["slot"], "device.card");
        let entry = ui["entry"].as_str().unwrap();
        builds.push(ui["build"].as_str().unwrap().to_owned());

        probe
            .client
            .publish(
                format!("{prefix}/manage/cmd"),
                rumqttc::QoS::AtLeastOnce,
                false,
                json!({"action":"ui_file","request_id":request_id,"file":entry}).to_string(),
            )
            .await
            .unwrap();
        let reply = until(
            &mut probe,
            &format!("{prefix}/manage/response"),
            |payload| {
                serde_json::from_slice::<Value>(payload)
                    .is_ok_and(|reply| reply["request_id"] == request_id)
            },
        )
        .await;
        let reply: Value = serde_json::from_slice(&reply.payload).unwrap();
        assert_eq!(reply["status"], "ok");
        assert_eq!(reply["done"], true);
        assert_eq!(
            STANDARD.decode(reply["data"].as_str().unwrap()).unwrap(),
            include_bytes!("../../../ui/ac-card/index.js")
        );
    }
    assert_eq!(builds[0], builds[1]);
    first.connected().await.assert_read_only().await;
    second.connected().await.assert_read_only().await;
}

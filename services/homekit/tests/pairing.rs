use homecore_test_support::{homecore, mqtt_broker};
use serde_json::json;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, SocketAddrV4},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Fixture {
    child: Child,
    root: PathBuf,
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
#[ignore = "requires Docker and the pinned homeCore image"]
async fn homecore_receives_typed_homekit_devices_from_a_fake_bridge() {
    let core = homecore::Core::start_for_plugin("plugin.teslemetry-powerwall").await;
    let root = core.root.join("homekit-contract");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let accessory_root = root.join("accessory");
    let state = root.join("state");
    fs::create_dir(&accessory_root).unwrap();
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(std::env::var("HAP_NODE_BIN").unwrap_or_else(|_| "node".into()))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hap/accessory.cjs"))
        .env("HAP_STORE_DIR", &accessory_root)
        .env("HAP_BRIDGE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port = line
        .trim()
        .strip_prefix("PORT=")
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let mut fixture = Fixture {
        child,
        root: root.clone(),
    };
    let txt = std::collections::HashMap::from([
        ("id".to_owned(), "02:11:22:33:44:55".to_owned()),
        ("sf".to_owned(), "1".to_owned()),
    ]);
    let discovered = hap_transport::discovery_test_support::parse_txt(
        "Fixture._hap._tcp.local.",
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        &txt,
    )
    .unwrap();
    homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered),
        "123-45-678",
        &state,
    )
    .await
    .unwrap();
    let devices = homecore_homekit::read_sensors_saved("02:11:22:33:44:55", &state)
        .await
        .unwrap();
    let climate = devices
        .iter()
        .find(|(_, state)| state.get("temperature").is_some())
        .unwrap()
        .0;
    let smoke = devices
        .iter()
        .find(|(_, state)| state.get("smoke").is_some())
        .unwrap()
        .0;
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let _service = Process(
        Command::new(env!("CARGO_BIN_EXE_homecore-homekit"))
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", core.mqtt_port.to_string())
            .env("MQTT_USERNAME", "plugin.teslemetry-powerwall")
            .env("MQTT_PASSWORD_FILE", core.root.join("mqtt-password"))
            .env("HOMEKIT_PLUGIN_ID", "plugin.teslemetry-powerwall")
            .env("HOMEKIT_STATE_DIR", &state)
            .env("HOMEKIT_UI_DIR", ui)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for (aid, expected_type) in [(climate, "temperature_sensor"), (smoke, "smoke_sensor")] {
        let path = format!("/devices/homekit_021122334455_{aid}");
        let device: serde_json::Value = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Ok(response) = core.get(&path).send().await {
                    if response.status().is_success() {
                        if let Ok(value) = response.json::<serde_json::Value>().await {
                            if value["device_type"] == expected_type {
                                break value;
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("homeCore did not receive device");
        assert_eq!(device["device_type"], expected_type);
    }
    let contact = devices
        .iter()
        .find(|(_, state)| state.get("open") == Some(&json!(true)))
        .unwrap()
        .0;
    writeln!(fixture.child.stdin.as_mut().unwrap(), "close").unwrap();
    let contact_path = format!("/devices/homekit_021122334455_{contact}");
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(response) = core.get(&contact_path).send().await {
                if let Ok(value) = response.json::<serde_json::Value>().await {
                    if value["attributes"]["open"] == false {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("contact update barrier");
    let offline = devices
        .iter()
        .find(|(_, state)| state.get("_offline") == Some(&json!(true)))
        .unwrap()
        .0;
    let path = format!("/devices/homekit_021122334455_{offline}");
    let device: serde_json::Value = core.get(&path).send().await.unwrap().json().await.unwrap();
    assert_eq!(device["device_type"], "temperature_sensor");
    assert_eq!(device["available"], false);
}

#[tokio::test]
async fn pairs_with_a_homekit_accessory_and_reads_its_contact_sensor() {
    let root = std::env::temp_dir().join(format!("homekit-fixture-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture_root = root.join("accessory");
    fs::create_dir(&fixture_root).unwrap();
    let state_root = root.join("controller");
    fs::create_dir(&state_root).unwrap();
    fs::set_permissions(&state_root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hap/accessory.cjs");
    let node = std::env::var("HAP_NODE_BIN").unwrap_or_else(|_| "node".into());
    let mut child = Command::new(node)
        .arg(fixture)
        .env("HAP_STORE_DIR", fixture_root)
        .env("HAP_CONTROLLER_DIR", &state_root)
        .env("HAP_DROP_VERIFY_NUMBER", "2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let mut fixture_process = Fixture { child, root };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let port = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("fixture did not start")
        .strip_prefix("PORT=")
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let txt = std::collections::HashMap::from([
        ("id".to_owned(), "02:11:22:33:44:55".to_owned()),
        ("sf".to_owned(), "1".to_owned()),
    ]);
    let discovered = hap_transport::discovery_test_support::parse_txt(
        "Fixture._hap._tcp.local.",
        address.into(),
        &txt,
    )
    .unwrap();
    assert!(homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered.clone()),
        "111-22-333",
        &state_root,
    )
    .await
    .is_err());
    assert!(homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered.clone()),
        "123-45-678",
        &state_root,
    )
    .await
    .is_err());
    fs::set_permissions(&state_root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered),
        "123-45-678",
        &state_root,
    )
    .await
    .is_err());
    let state = homecore_homekit::read_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert_eq!(state, json!({"open":true,"battery_pct":69}));
    assert_eq!(
        fs::metadata(state_root.join("pairing.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(state_root.join("pairing.json")).unwrap()).unwrap();
    assert_ne!(stored["controller"]["id"], "hap-rust-controller");
    let restored = homecore_homekit::read_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert_eq!(restored, json!({"open":true,"battery_pct":69}));
    let mut watch = homecore_homekit::watch_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert_eq!(watch.current(), &json!({"open":true,"battery_pct":69}));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "SUBSCRIBED"
    );
    writeln!(fixture_process.child.stdin.as_mut().unwrap(), "close").unwrap();
    let update = tokio::time::timeout(Duration::from_secs(5), watch.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(update, json!({"open":false,"battery_pct":69}));
}

#[tokio::test]
async fn reads_each_contact_from_a_bridge_independently() {
    let root = std::env::temp_dir().join(format!("homekit-bridge-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture_root = root.join("accessory");
    let state_root = root.join("controller");
    fs::create_dir(&fixture_root).unwrap();
    fs::create_dir(&state_root).unwrap();
    fs::set_permissions(&state_root, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(std::env::var("HAP_NODE_BIN").unwrap_or_else(|_| "node".into()))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hap/accessory.cjs"))
        .env("HAP_STORE_DIR", fixture_root)
        .env("HAP_BRIDGE", "1")
        .env("HAP_STRICT_CONTROLLER_ID", "1")
        .env("HAP_RACE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let mut fixture = Fixture { child, root };
    let mut line = String::new();
    let mut output = BufReader::new(output);
    output.read_line(&mut line).unwrap();
    let port = line
        .trim()
        .strip_prefix("PORT=")
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let txt = std::collections::HashMap::from([
        ("id".to_owned(), "02:11:22:33:44:55".to_owned()),
        ("sf".to_owned(), "1".to_owned()),
    ]);
    let discovered = hap_transport::discovery_test_support::parse_txt(
        "Fixture._hap._tcp.local.",
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        &txt,
    )
    .unwrap();
    homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered),
        "123-45-678",
        &state_root,
    )
    .await
    .unwrap();
    let readings = homecore_homekit::read_contacts_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert_eq!(readings.len(), 2);
    assert_ne!(readings[0].0, readings[1].0);
    assert_eq!(readings[0].1, json!({"open":true,"battery_pct":69}));
    assert_eq!(readings[1].1, json!({"open":false}));
    let sensors = homecore_homekit::read_sensors_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert!(
        sensors.iter().any(|(_, state)| *state
            == json!({
                "temperature": 21.5, "humidity": 42.0, "battery_pct": 83
            })),
        "actual readings: {sensors:?}"
    );
    assert!(
        sensors
            .iter()
            .any(|(_, state)| *state == json!({"smoke":true,"fault":false,"battery_pct":74})),
        "smoke detector missing: {sensors:?}"
    );
    assert!(
        sensors
            .iter()
            .any(|(_, state)| *state == json!({"_offline":true,"_type":"temperature_sensor"})),
        "unavailable climate sensor missing: {sensors:?}"
    );
    assert!(
        sensors
            .iter()
            .any(|(_, state)| *state == json!({"on":true,"in_use":false})),
        "outlet missing: {sensors:?}"
    );
    let invited_root = fixture.root.join("invited-controller");
    fs::create_dir(&invited_root).unwrap();
    fs::set_permissions(&invited_root, fs::Permissions::from_mode(0o700)).unwrap();
    homecore_homekit::invite_controller("02:11:22:33:44:55", &state_root, &invited_root)
        .await
        .unwrap();
    assert_eq!(
        homecore_homekit::read_contacts_saved("02:11:22:33:44:55", &invited_root)
            .await
            .unwrap(),
        readings
    );
    assert_eq!(
        homecore_homekit::read_contacts_saved("02:11:22:33:44:55", &state_root)
            .await
            .unwrap(),
        readings
    );
    let mut watch = homecore_homekit::watch_contacts_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert_eq!(watch.current()[0].1, json!({"open":false,"battery_pct":69}));
    assert_eq!(watch.current()[1], readings[1]);
    let outlet_id = sensors
        .iter()
        .find(|(_, state)| state.get("in_use").is_some())
        .unwrap()
        .0;
    watch.set_outlet(outlet_id, false).await.unwrap();
    let outlet_update = tokio::time::timeout(Duration::from_secs(5), watch.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        outlet_update,
        (outlet_id, json!({"on":false,"in_use":false}))
    );
    writeln!(fixture.child.stdin.as_mut().unwrap(), "open").unwrap();
    let update = tokio::time::timeout(Duration::from_secs(5), watch.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update,
        (readings[0].0, json!({"open":true,"battery_pct":69}))
    );
    writeln!(fixture.child.stdin.as_mut().unwrap(), "battery=50").unwrap();
    let update = tokio::time::timeout(Duration::from_secs(5), watch.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update,
        (readings[0].0, json!({"open":true,"battery_pct":50}))
    );
    drop(watch);
    let mut broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let publisher = tokio::spawn(homecore_homekit::run_contact_publisher(
        probe.client.clone(),
        "plugin.homekit".to_owned(),
        "02:11:22:33:44:55".to_owned(),
        state_root.clone(),
    ));
    let topic = format!(
        "homecore/devices/homekit_021122334455_{}/state",
        readings[0].0
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut contact_seen = false;
        let mut climate_seen = false;
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == topic
                && serde_json::from_slice::<serde_json::Value>(&msg.payload).unwrap()["open"]
                    == true
            {
                contact_seen = true;
            }
            if msg.topic.ends_with("/state")
                && serde_json::from_slice::<serde_json::Value>(&msg.payload)
                    .unwrap()
                    .get("temperature")
                    == Some(&json!(21.5))
            {
                climate_seen = true;
            }
            if contact_seen && climate_seen {
                break;
            }
        }
    })
    .await
    .unwrap();
    writeln!(fixture.child.stdin.as_mut().unwrap(), "close").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == topic
                && serde_json::from_slice::<serde_json::Value>(&msg.payload).unwrap()["open"]
                    == false
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    publisher.abort();
    let _ = publisher.await;
    while probe.messages.try_recv().is_ok() {}
    let password_file = fixture.root.join("mqtt-password");
    fs::write(&password_file, "fixture-password").unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_homecore-homekit"))
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", broker.port.to_string())
            .env("MQTT_USERNAME", "fixture-user")
            .env("MQTT_PASSWORD_FILE", password_file)
            .env("HOMEKIT_PLUGIN_ID", "plugin.homekit")
            .env("HOMEKIT_REFRESH_SECS", "1")
            .env("HOMEKIT_HEARTBEAT_SECS", "1")
            .env("HOMEKIT_STATE_DIR", &state_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == "homecore/plugins/plugin.homekit/register" {
                break;
            }
        }
    })
    .await
    .unwrap();
    let smoke_id = sensors
        .iter()
        .find(|(_, state)| state.get("smoke").is_some())
        .unwrap()
        .0;
    let smoke_availability =
        format!("homecore/devices/homekit_021122334455_{smoke_id}/availability");
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == smoke_availability && msg.payload.as_ref() == b"online" {
                break;
            }
        }
    })
    .await
    .expect("smoke detector must initially be online");
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == "homecore/plugins/fixture-user/heartbeat"
                && serde_json::from_slice::<serde_json::Value>(&msg.payload).unwrap()
                    ["device_count"]
                    == 6
            {
                break;
            }
        }
    })
    .await
    .expect("management heartbeat must count all six discovered devices");
    let climate_id = sensors
        .iter()
        .find(|(_, state)| state.get("temperature").is_some())
        .unwrap()
        .0;
    let climate_topic = format!("homecore/devices/homekit_021122334455_{climate_id}/state");
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == climate_topic {
                break;
            }
        }
    })
    .await
    .expect("unchanged sensor readings must be refreshed");
    let outlet_topic = format!("homecore/devices/homekit_021122334455_{outlet_id}");
    probe
        .client
        .publish(
            format!("{outlet_topic}/cmd"),
            rumqttc::QoS::AtLeastOnce,
            false,
            r#"{"on":true}"#,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == format!("{outlet_topic}/state")
                && serde_json::from_slice::<serde_json::Value>(&msg.payload)
                    .unwrap()
                    .get("on")
                    == Some(&json!(true))
            {
                break;
            }
        }
    })
    .await
    .expect("outlet command must be reflected in confirmed state");
    writeln!(fixture.child.stdin.as_mut().unwrap(), "smoke-offline").unwrap();
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == smoke_availability && msg.payload.as_ref() == b"offline" {
                break;
            }
        }
    })
    .await
    .expect("unavailable smoke detector must be marked offline");
    writeln!(fixture.child.stdin.as_mut().unwrap(), "smoke-online").unwrap();
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == smoke_availability && msg.payload.as_ref() == b"online" {
                break;
            }
        }
    })
    .await
    .expect("recovered smoke detector must be marked online");
    writeln!(fixture.child.stdin.as_mut().unwrap(), "open").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == topic
                && serde_json::from_slice::<serde_json::Value>(&msg.payload).unwrap()["open"]
                    == true
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    fixture.child.kill().unwrap();
    let availability = format!(
        "homecore/devices/homekit_021122334455_{}/availability",
        readings[0].0
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == availability && msg.payload.as_ref() == b"offline" {
                break;
            }
        }
    })
    .await
    .unwrap();
    fixture.child.wait().unwrap();
    let mut restarted =
        Command::new(std::env::var("HAP_NODE_BIN").unwrap_or_else(|_| "node".into()))
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hap/accessory.cjs"))
            .env("HAP_STORE_DIR", fixture.root.join("accessory"))
            .env("HAP_BRIDGE", "1")
            .env("HAP_PORT", port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
    let mut ready = String::new();
    let mut restarted_output = BufReader::new(restarted.stdout.take().unwrap());
    restarted_output.read_line(&mut ready).unwrap();
    assert_eq!(ready.trim(), format!("PORT={port}"));
    fixture.child = restarted;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == availability && msg.payload.as_ref() == b"online" {
                break;
            }
        }
    })
    .await
    .unwrap();
    drop(probe);
    broker.restart().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == availability && msg.payload.as_ref() == b"online" {
                break;
            }
        }
    })
    .await
    .unwrap();
    writeln!(fixture.child.stdin.as_mut().unwrap(), "close").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = probe.messages.recv().await.unwrap();
            if msg.topic == topic
                && serde_json::from_slice::<serde_json::Value>(&msg.payload).unwrap()["open"]
                    == false
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let mut crash_probe = mqtt_broker::Probe::start(broker.port).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let msg = crash_probe.messages.recv().await.unwrap();
            if msg.topic == smoke_availability && msg.payload.as_ref() == b"online" {
                break;
            }
        }
    })
    .await
    .expect("smoke detector should be online after broker restart");
    process.0.kill().unwrap();
    process.0.wait().unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let msg = crash_probe.messages.recv().await.unwrap();
            if msg.topic == smoke_availability && msg.payload.as_ref() == b"offline" {
                break;
            }
        }
    })
    .await
    .expect("killed controller must mark smoke detector offline");
}

#[tokio::test]
async fn lost_final_pairing_response_warns_that_recovery_may_be_required() {
    let root =
        std::env::temp_dir().join(format!("homekit-lost-confirmation-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture_root = root.join("accessory");
    let state_root = root.join("controller");
    fs::create_dir(&fixture_root).unwrap();
    fs::create_dir(&state_root).unwrap();
    fs::set_permissions(&state_root, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(std::env::var("HAP_NODE_BIN").unwrap_or_else(|_| "node".into()))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hap/accessory.cjs"))
        .env("HAP_STORE_DIR", fixture_root)
        .env("HAP_LOSE_M6", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let _fixture = Fixture { child, root };
    let mut line = String::new();
    BufReader::new(output).read_line(&mut line).unwrap();
    let port = line
        .trim()
        .strip_prefix("PORT=")
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let txt = std::collections::HashMap::from([
        ("id".to_owned(), "02:11:22:33:44:55".to_owned()),
        ("sf".to_owned(), "1".to_owned()),
    ]);
    let discovered = hap_transport::discovery_test_support::parse_txt(
        "Fixture._hap._tcp.local.",
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        &txt,
    )
    .unwrap();
    let result = homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered.clone()),
        "123-45-678",
        &state_root,
    )
    .await;
    assert_eq!(
        result.unwrap_err(),
        "Pairing confirmation was lost; accessory recovery may be needed"
    );
    let retry = homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered),
        "123-45-678",
        &state_root,
    )
    .await;
    assert_eq!(
        retry.unwrap_err(),
        "Pairing requires recovery before retrying"
    );
}

#[tokio::test]
async fn management_setup_pairs_the_discovered_fake_accessory() {
    tokio::task::LocalSet::new().run_until(async {
        let root=std::env::temp_dir().join(format!("homekit-setup-flow-{}",std::process::id()));
        fs::create_dir(&root).unwrap();fs::set_permissions(&root,fs::Permissions::from_mode(0o700)).unwrap();
        let accessory_root=root.join("accessory");let state_root=root.join("controller");
        fs::create_dir(&accessory_root).unwrap();fs::create_dir(&state_root).unwrap();fs::set_permissions(&state_root,fs::Permissions::from_mode(0o700)).unwrap();
        let mut child=Command::new(std::env::var("HAP_NODE_BIN").unwrap_or_else(|_|"node".into()))
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hap/accessory.cjs"))
            .env("HAP_STORE_DIR",accessory_root).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let output=child.stdout.take().unwrap();let _fixture=Fixture{child,root};let mut line=String::new();
        BufReader::new(output).read_line(&mut line).unwrap();
        let port=line.trim().strip_prefix("PORT=").unwrap().parse::<u16>().unwrap();
        let txt=std::collections::HashMap::from([("id".to_owned(),"02:11:22:33:44:55".to_owned()),("sf".to_owned(),"1".to_owned())]);
        let accessory=hap_transport::discovery_test_support::parse_txt("Fixture._hap._tcp.local.",SocketAddrV4::new(Ipv4Addr::LOCALHOST,port).into(),&txt).unwrap();
        let discover:homecore_homekit::setup::Discover=std::sync::Arc::new(move || {let accessory=accessory.clone();Box::pin(async move {Ok(vec![accessory])})});
        let broker=mqtt_broker::Broker::start().await;let mut probe=mqtt_broker::Probe::start(broker.port).await;
        let task=tokio::task::spawn_local(homecore_homekit::setup::run_with_discovery("127.0.0.1".into(),broker.port,"fixture-user".into(),"fixture-password".into(),state_root.clone(),std::path::PathBuf::from("ui"),discover));
        tokio::time::timeout(Duration::from_secs(10),async {loop {if probe.messages.recv().await.unwrap().topic=="homecore/plugins/fixture-user/capabilities" {break;}}}).await.unwrap();
        for command in [json!({"action":"discover_accessories","request_id":"discover"}),json!({"action":"pair_accessory","request_id":"pair","id":"02:11:22:33:44:55","pin":"123-45-678"}),json!({"action":"pair_accessory","request_id":"again","id":"02:11:22:33:44:55","pin":"123-45-678"})] {
            let request=command["request_id"].as_str().unwrap().to_owned();
            probe.client.publish("homecore/plugins/fixture-user/manage/cmd",rumqttc::QoS::AtLeastOnce,false,command.to_string()).await.unwrap();
            let reply=tokio::time::timeout(Duration::from_secs(20),async {loop {let p=probe.messages.recv().await.unwrap();if p.topic.contains("manage/") && !p.topic.ends_with("/cmd") {if let Ok(v)=serde_json::from_slice::<serde_json::Value>(&p.payload) {if v["request_id"]==request {break v;}}}}}).await.unwrap();
            if request == "again" { assert_eq!(reply["error"],"Controller already configured"); }
            else { assert_eq!(reply["status"],"ok"); }
        }
        assert_eq!(homecore_homekit::read_saved("02:11:22:33:44:55",&state_root).await.unwrap(),json!({"open":true,"battery_pct":69}));
        task.abort();
    }).await;
}

#[tokio::test]
async fn cancelled_pairing_requires_recovery_before_retry() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = std::env::temp_dir().join(format!("homekit-cancel-{}", std::process::id()));
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let accessory_root = root.join("accessory");
            let state_root = root.join("controller");
            fs::create_dir(&accessory_root).unwrap();
            fs::create_dir(&state_root).unwrap();
            fs::set_permissions(&state_root, fs::Permissions::from_mode(0o700)).unwrap();
            let mut child =
                Command::new(std::env::var("HAP_NODE_BIN").unwrap_or_else(|_| "node".into()))
                    .arg(
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/hap/accessory.cjs"),
                    )
                    .env("HAP_STORE_DIR", accessory_root)
                    .env("HAP_DELAY_M6", "1")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
            let output = child.stdout.take().unwrap();
            let _fixture = Fixture { child, root };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(output).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            });
            let port = rx
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .strip_prefix("PORT=")
                .unwrap()
                .parse::<u16>()
                .unwrap();
            let txt = std::collections::HashMap::from([
                ("id".to_owned(), "02:11:22:33:44:55".to_owned()),
                ("sf".to_owned(), "1".to_owned()),
            ]);
            let accessory = hap_transport::discovery_test_support::parse_txt(
                "Fixture._hap._tcp.local.",
                SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
                &txt,
            )
            .unwrap();
            let copy = accessory.clone();
            let state = state_root.clone();
            let task = tokio::task::spawn_local(async move {
                homecore_homekit::pair_and_read(
                    hap_controller::Discovered::Ip(copy),
                    "123-45-678",
                    &state,
                )
                .await
            });
            let signal =
                tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(10)))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(signal, "WAIT_M6");
            task.abort();
            let _ = task.await;
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                homecore_homekit::pair_and_read(
                    hap_controller::Discovered::Ip(accessory),
                    "123-45-678",
                    &state_root,
                ),
            )
            .await
            .expect("retry must be rejected immediately");
            assert_eq!(
                result.unwrap_err(),
                "Pairing requires recovery before retrying"
            );
        })
        .await;
}

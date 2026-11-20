#[allow(dead_code)]
#[path = "support/mqtt_broker.rs"]
mod mqtt_broker;
use rbroadlink::{network::CommandMessage, traits::CommandTrait};
use std::{
    net::UdpSocket,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

struct Reply;
impl CommandTrait for Reply {
    fn packet_type() -> u16 {
        0x6a
    }
}
const INITIAL: [u8; 16] = [
    0x09, 0x76, 0x28, 0x34, 0x3f, 0xe9, 0x9e, 0x23, 0x76, 0x5c, 0x15, 0x13, 0xac, 0xcf, 0x8b, 0x02,
];
const KEY: [u8; 16] = [7; 16];
const MAC: [u8; 6] = [2, 3, 4, 5, 6, 7];
struct Owned(Child, std::path::PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_file(&self.1);
    }
}

#[tokio::test]
#[ignore = "requires the pinned local Mosquitto image and Docker"]
async fn device_status_is_published_to_homecore() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let address = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let (_, peer) = server.recv_from(&mut buf).expect("auth request");
        let count = u16::from_le_bytes([buf[40], buf[41]]);
        let mut auth = vec![1, 2, 3, 4];
        auth.extend_from_slice(&KEY);
        server
            .send_to(
                &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                    .pack_with_payload(&auth, &INITIAL)
                    .unwrap(),
                peer,
            )
            .unwrap();
        let (n, peer) = server.recv_from(&mut buf).expect("status request");
        let request = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &KEY).unwrap();
        assert_eq!(&request[..2], &[0x0e, 0]);
        let count = u16::from_le_bytes([buf[40], buf[41]]);
        let data = br#"{"envtemp":22,"temp":24,"ac_pwr":1,"ac_mode":0,"ac_mark":3,"ac_vdir":1}"#;
        let mut frame = vec![0; 14];
        frame[10..12].copy_from_slice(&(data.len() as u16).to_le_bytes());
        frame.extend_from_slice(data);
        let checksum = frame[8..]
            .iter()
            .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
        frame[6..8].copy_from_slice(&checksum.to_le_bytes());
        server
            .send_to(
                &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                    .pack_with_payload(&frame, &KEY)
                    .unwrap(),
                peer,
            )
            .unwrap();
    });
    let path = std::env::temp_dir().join(format!("electrolux-mqtt-pass-{}", std::process::id()));
    std::fs::write(&path, "fixture-password").unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_electrolux-bridge"))
        .env("ELECTROLUX_ADDRESS", address.to_string())
        .env("ELECTROLUX_MAC", "020304050607")
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _owned = Owned(child, path);
    let state = tokio::time::timeout(Duration::from_secs(4), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/electrolux_living_room/state" {
                return serde_json::from_slice::<serde_json::Value>(&message.payload).unwrap();
            }
        }
        panic!("MQTT probe closed")
    })
    .await
    .expect("AC state should be published");
    assert_eq!(state["temperature"], 22.0);
    assert_eq!(state["setpoint"], 24.0);
    assert_eq!(state["hvac_mode"], "cool");
    worker.join().unwrap();
}

#[tokio::test]
#[ignore = "requires the pinned local Mosquitto image and Docker"]
async fn setpoint_command_reaches_ac_and_publishes_reported_state() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    let address = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let mut setpoint = 24;
        loop {
            let (n, peer) = server.recv_from(&mut buf).unwrap();
            let count = u16::from_le_bytes([buf[40], buf[41]]);
            if buf[38] == 0x65 {
                let mut auth = vec![1, 2, 3, 4];
                auth.extend_from_slice(&KEY);
                server
                    .send_to(
                        &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                            .pack_with_payload(&auth, &INITIAL)
                            .unwrap(),
                        peer,
                    )
                    .unwrap();
                continue;
            }
            let request = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &KEY).unwrap();
            let payload = if request[0] == 0x17 {
                assert_eq!(&request[14..], br#"{"temp":25}"#);
                setpoint = 25;
                "{}".to_string()
            } else {
                assert_eq!(request[0], 0x0e);
                format!(
                    r#"{{"envtemp":22,"temp":{setpoint},"ac_pwr":1,"ac_mode":0,"ac_mark":3,"ac_vdir":1}}"#
                )
            };
            let mut frame = vec![0; 14];
            frame[10..12].copy_from_slice(&(payload.len() as u16).to_le_bytes());
            frame.extend_from_slice(payload.as_bytes());
            let checksum = frame[8..]
                .iter()
                .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
            frame[6..8].copy_from_slice(&checksum.to_le_bytes());
            server
                .send_to(
                    &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                        .pack_with_payload(&frame, &KEY)
                        .unwrap(),
                    peer,
                )
                .unwrap();
            if setpoint == 25 && request[0] == 0x0e {
                break;
            }
        }
    });
    let path = std::env::temp_dir().join(format!("electrolux-command-pass-{}", std::process::id()));
    std::fs::write(&path, "fixture-password").unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_electrolux-bridge"))
        .env("ELECTROLUX_ADDRESS", address.to_string())
        .env("ELECTROLUX_MAC", "020304050607")
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _owned = Owned(child, path);
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/electrolux_living_room/state" {
                break;
            }
        }
    })
    .await
    .expect("initial state");
    probe
        .client
        .publish(
            "homecore/devices/electrolux_living_room/cmd",
            rumqttc::QoS::AtLeastOnce,
            false,
            br#"{"setpoint":25}"#.as_slice(),
        )
        .await
        .unwrap();
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/electrolux_living_room/state" {
                let value: serde_json::Value = serde_json::from_slice(&message.payload).unwrap();
                if value["setpoint"] == 25.0 {
                    return value;
                }
            }
        }
        panic!("probe closed")
    })
    .await
    .expect("updated AC state");
    assert_eq!(state["setpoint"], 25.0);
    worker.join().unwrap();
}

#[tokio::test]
#[ignore = "requires the pinned local Mosquitto image and Docker"]
async fn homecore_controls_follow_reported_device_state() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let address = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let mut device = serde_json::json!({"envtemp":22,"temp":24,"ac_pwr":1,"ac_mode":0,"ac_mark":3,"ac_vdir":1});
        let mut changes = 0;
        loop {
            let (n, peer) = server.recv_from(&mut buf).unwrap();
            let count = u16::from_le_bytes([buf[40], buf[41]]);
            if buf[38] == 0x65 {
                let mut auth = vec![1, 2, 3, 4];
                auth.extend_from_slice(&KEY);
                server
                    .send_to(
                        &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                            .pack_with_payload(&auth, &INITIAL)
                            .unwrap(),
                        peer,
                    )
                    .unwrap();
                continue;
            }
            let request = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &KEY).unwrap();
            let payload = if request[0] == 0x0e {
                device.to_string()
            } else {
                let command: serde_json::Value = serde_json::from_slice(&request[14..]).unwrap();
                let obj = command.as_object().unwrap();
                assert_eq!(obj.len(), 1);
                let (key, value) = obj.iter().next().unwrap();
                assert!(matches!(
                    key.as_str(),
                    "ac_pwr" | "ac_mode" | "ac_mark" | "ac_vdir"
                ));
                device[key] = value.clone();
                changes += 1;
                "{}".to_owned()
            };
            let mut frame = vec![0; 14];
            frame[10..12].copy_from_slice(&(payload.len() as u16).to_le_bytes());
            frame.extend_from_slice(payload.as_bytes());
            let checksum = frame[8..]
                .iter()
                .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
            frame[6..8].copy_from_slice(&checksum.to_le_bytes());
            server
                .send_to(
                    &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                        .pack_with_payload(&frame, &KEY)
                        .unwrap(),
                    peer,
                )
                .unwrap();
            if changes == 5 && request[0] == 0x0e {
                break;
            }
        }
    });
    let path =
        std::env::temp_dir().join(format!("electrolux-controls-pass-{}", std::process::id()));
    std::fs::write(&path, "fixture-password").unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_electrolux-bridge"))
        .env("ELECTROLUX_ADDRESS", address.to_string())
        .env("ELECTROLUX_MAC", "020304050607")
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _owned = Owned(child, path);
    let cases = [
        (r#"{"on":false}"#, "hvac_mode", serde_json::json!("off")),
        (r#"{"on":true}"#, "hvac_mode", serde_json::json!("cool")),
        (
            r#"{"hvac_mode":"heat"}"#,
            "hvac_mode",
            serde_json::json!("heat"),
        ),
        (
            r#"{"fan_mode":"quiet"}"#,
            "fan_mode",
            serde_json::json!("quiet"),
        ),
        (r#"{"swing":false}"#, "swing", serde_json::json!(false)),
    ];
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(m) = probe.messages.recv().await {
            if m.topic == "homecore/devices/electrolux_living_room/state" {
                return;
            }
        }
        panic!("probe closed")
    })
    .await
    .expect("initial state");
    for (command, key, expected) in cases {
        probe
            .client
            .publish(
                "homecore/devices/electrolux_living_room/cmd",
                rumqttc::QoS::AtLeastOnce,
                false,
                command,
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(4), async {
            while let Some(m) = probe.messages.recv().await {
                if m.topic == "homecore/devices/electrolux_living_room/state" {
                    let state: serde_json::Value = serde_json::from_slice(&m.payload).unwrap();
                    if state[key] == expected {
                        return;
                    }
                }
            }
            panic!("probe closed");
        })
        .await
        .expect("reported state after command");
    }
    worker.join().unwrap();
}

#[tokio::test]
#[ignore = "requires the pinned local Mosquitto image and Docker"]
async fn broker_restart_restores_device_registration_and_status() {
    let mut broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let address = server.local_addr().unwrap();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = done.clone();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        while !stop.load(std::sync::atomic::Ordering::SeqCst) {
            let Ok((n, peer)) = server.recv_from(&mut buf) else {
                continue;
            };
            let count = u16::from_le_bytes([buf[40], buf[41]]);
            let (payload, key) = if buf[38] == 0x65 {
                let mut auth = vec![1, 2, 3, 4];
                auth.extend_from_slice(&KEY);
                (auth, INITIAL)
            } else {
                let request = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &KEY).unwrap();
                assert_eq!(request[0], 0x0e);
                let data =
                    br#"{"envtemp":22,"temp":24,"ac_pwr":1,"ac_mode":0,"ac_mark":3,"ac_vdir":1}"#;
                let mut frame = vec![0; 14];
                frame[10..12].copy_from_slice(&(data.len() as u16).to_le_bytes());
                frame.extend_from_slice(data);
                let checksum = frame[8..]
                    .iter()
                    .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
                frame[6..8].copy_from_slice(&checksum.to_le_bytes());
                (frame, KEY)
            };
            server
                .send_to(
                    &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                        .pack_with_payload(&payload, &key)
                        .unwrap(),
                    peer,
                )
                .unwrap();
        }
    });
    let path =
        std::env::temp_dir().join(format!("electrolux-reconnect-pass-{}", std::process::id()));
    std::fs::write(&path, "fixture-password").unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_electrolux-bridge"))
        .env("ELECTROLUX_ADDRESS", address.to_string())
        .env("ELECTROLUX_MAC", "020304050607")
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _owned = Owned(child, path);
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(m) = probe.messages.recv().await {
            if m.topic == "homecore/devices/electrolux_living_room/state" {
                return;
            }
        }
        panic!("probe closed")
    })
    .await
    .expect("initial state");
    broker.restart().await;
    drop(probe);
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let (registration, state) = tokio::time::timeout(Duration::from_secs(12), async {
        let mut registration = false;
        let mut state = false;
        while let Some(m) = probe.messages.recv().await {
            registration |= m.topic == "homecore/plugins/plugin.electrolux.living_room/register";
            state |= m.topic == "homecore/devices/electrolux_living_room/state";
            if registration && state {
                return (registration, state);
            }
        }
        panic!("probe closed")
    })
    .await
    .expect("registration and state after broker restart");
    assert!(registration && state);
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    worker.join().unwrap();
}

#[tokio::test]
#[ignore = "requires the pinned local Mosquitto image and Docker"]
async fn electrolux_advertises_and_serves_the_shared_ac_card() {
    use base64::Engine;
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let path = std::env::temp_dir().join(format!("electrolux-ui-pass-{}", std::process::id()));
    std::fs::write(&path, "fixture-password").unwrap();
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/ac-card");
    let child = Command::new(env!("CARGO_BIN_EXE_electrolux-bridge"))
        .env("ELECTROLUX_ADDRESS", "127.0.0.1:9")
        .env("ELECTROLUX_MAC", "020304050607")
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", broker.port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", &path)
        .env("HC_PLUGIN_UI_DIR", &ui)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _owned = Owned(child, path);
    let caps = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(m) = probe.messages.recv().await {
            if m.topic == "homecore/plugins/plugin.electrolux.living_room/capabilities" {
                return serde_json::from_slice::<serde_json::Value>(&m.payload).unwrap();
            }
        }
        panic!("probe closed")
    })
    .await
    .expect("shared card capabilities");
    assert_eq!(caps["ui"]["entry"], "index.js");
    assert!(caps["ui"]["slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["element"] == "hc-ac-card"));
    let request = serde_json::json!({"action":"ui_file","request_id":"12345678-1234-4234-8234-123456789abc","file":"index.js","offset":0,"max":131072});
    probe
        .client
        .publish(
            "homecore/plugins/plugin.electrolux.living_room/manage/cmd",
            rumqttc::QoS::AtLeastOnce,
            false,
            request.to_string(),
        )
        .await
        .unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(m) = probe.messages.recv().await {
            if m.topic == "homecore/plugins/plugin.electrolux.living_room/manage/response" {
                return serde_json::from_slice::<serde_json::Value>(&m.payload).unwrap();
            }
        }
        panic!("probe closed")
    })
    .await
    .expect("shared card response");
    assert_eq!(reply["status"], "ok");
    assert_eq!(reply["done"], true);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(reply["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes, std::fs::read(ui.join("index.js")).unwrap());
}

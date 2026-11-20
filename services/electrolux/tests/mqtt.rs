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

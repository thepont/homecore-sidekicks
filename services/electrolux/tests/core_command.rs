#[path = "support/homecore.rs"]
mod homecore;
use rbroadlink::{network::CommandMessage, traits::CommandTrait};
use serde_json::{json, Value};
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
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
#[ignore = "requires Docker and pinned homeCore image"]
async fn homecore_api_power_command_changes_reported_device_state() {
    let core = homecore::Core::start().await;
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(12)))
        .unwrap();
    let address = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let mut power = 0;
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
            let payload = if request[0] == 0x18 {
                assert_eq!(&request[14..], br#"{"ac_pwr":1}"#);
                power = 1;
                "{}".to_string()
            } else {
                assert_eq!(request[0], 0x0e);
                format!(
                    r#"{{"envtemp":22,"temp":24,"ac_pwr":{power},"ac_mode":0,"ac_mark":3,"ac_vdir":1}}"#
                )
            };
            let mut frame = vec![0; 14];
            frame[10..12].copy_from_slice(&(payload.len() as u16).to_le_bytes());
            frame.extend_from_slice(payload.as_bytes());
            let sum = frame[8..]
                .iter()
                .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
            frame[6..8].copy_from_slice(&sum.to_le_bytes());
            server
                .send_to(
                    &CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
                        .pack_with_payload(&frame, &KEY)
                        .unwrap(),
                    peer,
                )
                .unwrap();
            if power == 1 && request[0] == 0x0e {
                break;
            }
        }
    });
    let _adapter = Owned(
        Command::new(env!("CARGO_BIN_EXE_electrolux-bridge"))
            .env("ELECTROLUX_ADDRESS", address.to_string())
            .env("ELECTROLUX_MAC", "020304050607")
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", core.mqtt_port.to_string())
            .env("MQTT_USERNAME", "plugin.electrolux.living_room")
            .env("MQTT_PASSWORD_FILE", core.root.join("mqtt-password"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let path = "/devices/electrolux_living_room";
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Ok(r) = core.get(path).send().await {
                if r.status().is_success() {
                    let d: Value = r.json().await.unwrap();
                    if d["available"] == true {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("device ready");
    let response = core
        .patch(&format!("{path}/state"))
        .json(&json!({"on":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
    let state = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let d: Value = core.get(path).send().await.unwrap().json().await.unwrap();
            if d["attributes"]["on"] == true {
                break d;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("device should report on after API command");
    assert_eq!(state["attributes"]["hvac_mode"], "cool");
    worker.join().unwrap();
}

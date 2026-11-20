use homecore_electrolux::transport::Client;
use rbroadlink::{network::CommandMessage, traits::CommandTrait};
use std::{net::UdpSocket, thread, time::Duration};

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

#[test]
fn authenticates_and_reads_a_status_through_broadlink() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let addr = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let (n, peer) = server.recv_from(&mut buf).unwrap();
        let auth = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &INITIAL).unwrap();
        assert!(auth.len() >= 48);
        let count = u16::from_le_bytes([buf[40], buf[41]]);
        let mut auth_response = vec![1, 2, 3, 4];
        auth_response.extend_from_slice(&KEY);
        let response = CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
            .pack_with_payload(&auth_response, &INITIAL)
            .unwrap();
        server.send_to(&response, peer).unwrap();
        let (n, peer) = server.recv_from(&mut buf).unwrap();
        let query = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &KEY).unwrap();
        assert_eq!(query, homecore_electrolux::protocol::status_request());
        let count = u16::from_le_bytes([buf[40], buf[41]]);
        let payload = br#"{"envtemp":22,"temp":24,"ac_pwr":1,"ac_mode":0,"ac_mark":3,"ac_vdir":1}"#;
        let mut frame = vec![0; 14];
        frame[10..12].copy_from_slice(&(payload.len() as u16).to_le_bytes());
        frame.extend_from_slice(payload);
        let sum = frame[8..]
            .iter()
            .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
        frame[6..8].copy_from_slice(&sum.to_le_bytes());
        let response = CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
            .pack_with_payload(&frame, &KEY)
            .unwrap();
        server.send_to(&response, peer).unwrap();
    });
    let mut client = Client::connect(addr, MAC, Duration::from_secs(2)).unwrap();
    assert_eq!(client.status().unwrap().mode, "cool");
    worker.join().unwrap();
}

#[test]
fn authentication_is_bounded_when_device_does_not_reply() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let start = std::time::Instant::now();
    assert!(Client::connect(
        server.local_addr().unwrap(),
        MAC,
        Duration::from_millis(100)
    )
    .is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn rejects_a_response_with_wrong_sequence() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let (_, peer) = server.recv_from(&mut buf).unwrap();
        let count = u16::from_le_bytes([buf[40], buf[41]]).wrapping_add(1);
        let mut payload = vec![1, 2, 3, 4];
        payload.extend_from_slice(&KEY);
        let response = CommandMessage::with_count::<Reply>(count, 0x4f9b, MAC, 0)
            .pack_with_payload(&payload, &INITIAL)
            .unwrap();
        server.send_to(&response, peer).unwrap();
    });
    assert!(Client::connect(addr, MAC, Duration::from_secs(2)).is_err());
    worker.join().unwrap();
}

#[test]
fn sends_power_command_to_authenticated_device() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let addr = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut buf = [0; 8192];
        let (_, peer) = server.recv_from(&mut buf).unwrap();
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
        let (n, peer) = server.recv_from(&mut buf).unwrap();
        let command = CommandMessage::unpack_with_payload(buf[..n].to_vec(), &KEY).unwrap();
        assert_eq!(&command[..2], &[0x18, 0]);
        assert_eq!(&command[14..], br#"{"ac_pwr":1}"#);
        let count = u16::from_le_bytes([buf[40], buf[41]]);
        let mut frame = vec![0; 14];
        frame[10] = 2;
        frame.extend_from_slice(b"{}");
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
    });
    let mut client = Client::connect(addr, MAC, Duration::from_secs(2)).unwrap();
    assert!(client.set_power(true).is_ok());
    worker.join().unwrap();
}

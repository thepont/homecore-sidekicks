use homecore_electrolux::transport::Client;
use std::{net::SocketAddr, time::Duration};

#[test]
#[ignore = "requires ELECTROLUX_LIVE_CONFIG and access to a real controller; read-only"]
fn real_controller_reports_a_valid_climate_status() {
    let path = std::env::var("ELECTROLUX_LIVE_CONFIG").expect("private live fixture path");
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("read live fixture"))
            .expect("valid live fixture");
    let host = data["host"].as_str().expect("controller host");
    let raw = data["mac"].as_str().expect("controller MAC");
    assert_eq!(raw.len(), 12, "MAC format");
    let mut mac = [0; 6];
    for (i, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16).expect("MAC format");
    }
    let address: SocketAddr = format!("{host}:80")
        .parse()
        .expect("IPv4 controller endpoint");
    let mut client =
        Client::connect(address, mac, Duration::from_secs(3)).expect("controller authentication");
    let state = client.status().expect("valid status from controller");
    assert!(state.current_temperature.is_finite());
}

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
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
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
    let mut watch = homecore_homekit::watch_contacts_saved("02:11:22:33:44:55", &state_root)
        .await
        .unwrap();
    assert_eq!(watch.current()[0].1, json!({"open":false,"battery_pct":69}));
    assert_eq!(watch.current()[1], readings[1]);
    writeln!(fixture.child.stdin.as_mut().unwrap(), "open").unwrap();
    let update = tokio::time::timeout(Duration::from_secs(5), watch.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update,
        (readings[0].0, json!({"open":true,"battery_pct":69}))
    );
}

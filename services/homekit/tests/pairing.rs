use serde_json::json;
use std::{
    fs,
    io::{BufRead, BufReader},
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
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let _fixture = Fixture { child, root };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if let Some(port) = line
                .strip_prefix("PORT=")
                .and_then(|s| s.parse::<u16>().ok())
            {
                let _ = tx.send(port);
                break;
            }
        }
    });
    let port = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("fixture did not start");
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
    let state = homecore_homekit::pair_and_read(
        hap_controller::Discovered::Ip(discovered),
        "123-45-678",
        &state_root,
    )
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
}

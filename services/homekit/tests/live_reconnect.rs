use hap_controller::{JsonFileStore, PairingStore};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    time::Duration,
};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("pairing.json"));
        let _ = fs::remove_file(self.0.join("pairing.json.tmp"));
        let _ = fs::remove_dir(&self.0);
    }
}

#[tokio::test]
#[ignore = "requires the paired Xiaomi trial hub and local mDNS"]
async fn reconnects_after_the_saved_endpoint_changes() {
    let source = PathBuf::from(
        std::env::var_os("HOMEKIT_LIVE_TEST_STORE").expect("trial pairing directory"),
    );
    let root = std::env::temp_dir().join(format!("homekit-reconnect-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let scratch = Scratch(root);
    let mut pairing: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("pairing.json")).unwrap()).unwrap();
    pairing["accessories"][0]["transport"]["addr"] = "127.0.0.1:1".into();
    let path = scratch.0.join("pairing.json");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(&serde_json::to_vec(&pairing).unwrap())
        .unwrap();
    let store = JsonFileStore::new(path);
    let id = store
        .load_pairings()
        .await
        .unwrap()
        .remove(0)
        .pairing
        .pairing_id;
    let states = tokio::time::timeout(
        Duration::from_secs(20),
        homecore_homekit::read_sensors_saved(&id, &scratch.0),
    )
    .await
    .expect("reconnect deadline")
    .expect("rediscover paired bridge");
    assert!(states.iter().any(|(_, state)| state.get("open").is_some()));
}

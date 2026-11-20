use base64::Engine;
use homecore_teslemetry::ui::UiAssets;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const REQUEST_ID: &str = "123e4567-e89b-42d3-a456-426614174001";

struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        let mut id = [0u8; 16];
        SystemRandom::new().fill(&mut id).unwrap();
        let id = id.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let path = std::env::temp_dir().join(format!("teslemetry-ui-{id}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn with_script(script: &[u8]) -> Self {
        let dir = Self::new();
        fs::write(dir.0.join("manifest.json"), manifest().to_string()).unwrap();
        fs::write(dir.0.join("index.js"), script).unwrap();
        dir
    }
    fn load(&self) -> UiAssets {
        UiAssets::load(&self.0, "0.1.0").expect("valid UI assets must load")
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn manifest() -> Value {
    json!({"host_api":1,"entry":"index.js","slots":[{
        "slot":"device.card","element":"hc-teslemetry-card",
        "device_types":["sensor"],"forms":["compact","standard","large"]
    }]})
}
fn request(file: &str, offset: u64, max: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({"action":"ui_file","request_id":REQUEST_ID,
        "file":file,"offset":offset,"max":max}))
    .unwrap()
}

#[test]
fn a_disk_bundle_declares_its_card_and_build_changes_with_content() {
    let dir = Folder::with_script(b"export const x=1;");
    let ui = dir.load().descriptor();
    assert_eq!(ui["host_api"], 1);
    assert_eq!(ui["entry"], "index.js");
    assert_eq!(ui["version"], "0.1.0");
    assert_eq!(ui["slots"], manifest()["slots"]);
    let build = ui["build"].as_str().unwrap();
    assert_eq!(build.len(), 16);
    assert!(build.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(dir.load().descriptor()["build"], build);
    fs::write(dir.0.join("index.js"), b"export const x=2;").unwrap();
    assert_ne!(dir.load().descriptor()["build"], build);
}

#[test]
fn management_chunks_reconstruct_the_file_and_report_completion() {
    let dir = Folder::with_script(b"123456789ABCDEFGHIJK");
    let assets = dir.load();
    let mut bytes = Vec::new();
    for (offset, done) in [(0, false), (7, false), (14, true)] {
        let reply = assets
            .respond(&request("index.js", offset, 7))
            .expect("reply");
        assert_eq!(reply["request_id"], REQUEST_ID);
        assert_eq!(reply["status"], "ok");
        assert_eq!(reply["done"], done);
        bytes.extend(
            base64::engine::general_purpose::STANDARD
                .decode(reply["data"].as_str().unwrap())
                .unwrap(),
        );
    }
    assert_eq!(bytes, b"123456789ABCDEFGHIJK");
    let end = assets.respond(&request("index.js", 20, 7)).unwrap();
    assert_eq!(end["done"], true);
    assert_eq!(end["data"], "");
    let default = assets
        .respond(
            &serde_json::to_vec(
                &json!({"request_id":REQUEST_ID,"action":"ui_file","file":"manifest.json"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(default["status"], "ok");
    assert_eq!(default["done"], true);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(default["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&decoded).unwrap(),
        manifest()
    );
    let ping = assets
        .respond(&serde_json::to_vec(&json!({"request_id":REQUEST_ID,"action":"ping"})).unwrap())
        .unwrap();
    assert_eq!(ping, json!({"request_id":REQUEST_ID,"status":"ok"}));
}

#[test]
fn invalid_manifests_paths_and_large_assets_are_not_advertised() {
    let dir = Folder::with_script(b"export {};");
    let original = manifest();
    let mut invalid = vec![json!({}), json!([]), json!(null)];
    for (field, value) in [
        ("host_api", json!(2)),
        ("entry", json!("../private")),
        ("entry", json!("other.js")),
        ("slots", json!([])),
    ] {
        let mut m = original.clone();
        m[field] = value;
        invalid.push(m);
    }
    for (field, value) in [
        ("slot", json!("plugin.page")),
        ("element", json!("script")),
        ("device_types", json!(["light"])),
        ("forms", json!(["huge"])),
    ] {
        let mut m = original.clone();
        m["slots"][0][field] = value;
        invalid.push(m);
    }
    for value in invalid {
        fs::write(dir.0.join("manifest.json"), value.to_string()).unwrap();
        assert!(UiAssets::load(&dir.0, "0.1.0").is_err(), "accepted {value}");
    }
    fs::write(dir.0.join("manifest.json"), original.to_string()).unwrap();
    for version in ["", " ", "bad\nversion"] {
        assert!(UiAssets::load(&dir.0, version).is_err());
    }
    fs::write(dir.0.join("index.js"), vec![b'x'; 512 * 1024 + 1]).unwrap();
    assert!(UiAssets::load(&dir.0, "0.1.0").is_err());
    fs::write(dir.0.join("index.js"), b"export {};").unwrap();
    fs::write(dir.0.join("manifest.json"), vec![b' '; 16 * 1024 + 1]).unwrap();
    assert!(UiAssets::load(&dir.0, "0.1.0").is_err());
    fs::remove_file(dir.0.join("manifest.json")).unwrap();
    let error = UiAssets::load(&dir.0, "0.1.0").unwrap_err();
    assert!(!error.to_string().contains(dir.0.to_str().unwrap()));
}

#[cfg(unix)]
#[test]
fn symlinks_directories_and_nonregular_assets_are_rejected() {
    use std::os::unix::fs::symlink;
    let dir = Folder::with_script(b"export {};");
    let outside = Folder::with_script(b"private bytes");
    fs::remove_file(dir.0.join("index.js")).unwrap();
    symlink(outside.0.join("index.js"), dir.0.join("index.js")).unwrap();
    assert!(UiAssets::load(&dir.0, "0.1.0").is_err());
    fs::remove_file(dir.0.join("index.js")).unwrap();
    fs::create_dir(dir.0.join("index.js")).unwrap();
    assert!(UiAssets::load(&dir.0, "0.1.0").is_err());
    let container = Folder::new();
    symlink(&outside.0, container.0.join("linked")).unwrap();
    assert!(UiAssets::load(&container.0.join("linked"), "0.1.0").is_err());
}

#[test]
fn requests_are_bounded_typed_and_confined_to_the_declared_bundle() {
    use homecore_teslemetry::ui::{respond_unavailable, MAX_REQUEST_BYTES};
    let dir = Folder::with_script(b"export {};");
    let assets = dir.load();
    for payload in [
        b"not json".to_vec(),
        b"{}".to_vec(),
        b"[]".to_vec(),
        vec![b' '; MAX_REQUEST_BYTES + 1],
        serde_json::to_vec(&json!({"request_id":"unsafe-id","action":"ping"})).unwrap(),
    ] {
        assert!(assets.respond(&payload).is_none());
        assert!(respond_unavailable(&payload).is_none());
    }
    let valid =
        json!({"request_id":REQUEST_ID,"action":"ui_file","file":"index.js","offset":0,"max":7});
    for (field, value) in [
        ("action", json!(null)),
        ("action", json!("set_config")),
        ("file", json!("../private")),
        ("file", json!("/etc/passwd")),
        ("file", json!("other.js")),
        ("file", json!(null)),
        ("max", json!(0)),
        ("max", json!(131073)),
        ("max", json!(-1)),
        ("max", json!("7")),
        ("offset", json!("0")),
        ("offset", json!(-1)),
        ("offset", json!(1.5)),
        ("offset", json!(u64::MAX)),
    ] {
        let mut cmd = valid.clone();
        cmd[field] = value;
        let bytes = serde_json::to_vec(&cmd).unwrap();
        for reply in [assets.respond(&bytes), respond_unavailable(&bytes)] {
            let reply = reply.expect("a valid correlation ID receives a rejection");
            assert_eq!(reply["status"], "error", "accepted {cmd}");
            assert!(reply.get("data").is_none());
        }
    }
    let ping = serde_json::to_vec(&json!({"action":"ping","request_id":REQUEST_ID})).unwrap();
    assert_eq!(respond_unavailable(&ping).unwrap()["status"], "ok");
    let reply = respond_unavailable(&serde_json::to_vec(&valid).unwrap()).unwrap();
    assert_eq!(reply["status"], "error");
}

#[test]
fn changed_or_removed_assets_are_rejected_instead_of_serving_a_cached_body() {
    let dir = Folder::with_script(b"original script");
    let assets = dir.load();
    fs::write(dir.0.join("index.js"), b"modified script").unwrap();
    let reply = assets.respond(&request("index.js", 0, 128)).unwrap();
    assert_eq!(reply["status"], "error");
    // Core's HTTP adapter maps this management-protocol phrase to 404.
    assert_eq!(reply["error"], "no such file");
    assert!(reply.get("data").is_none());
    fs::remove_file(dir.0.join("index.js")).unwrap();
    assert_eq!(
        assets.respond(&request("index.js", 0, 128)).unwrap()["status"],
        "error"
    );
    #[cfg(unix)]
    {
        let outside = Folder::with_script(b"private secret");
        std::os::unix::fs::symlink(outside.0.join("index.js"), dir.0.join("index.js")).unwrap();
        assert_eq!(
            assets.respond(&request("index.js", 0, 128)).unwrap()["status"],
            "error"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn large_transfers_leave_no_open_files_between_requests() {
    use homecore_teslemetry::ui::MAX_CHUNK_BYTES;
    let data = vec![b'q'; 512 * 1024];
    let dir = Folder::with_script(&data);
    let assets = dir.load();
    let held_files = || {
        fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|fd| fs::read_link(fd.path()).ok())
            .filter(|path| path.starts_with(&dir.0))
            .count()
    };
    assert_eq!(held_files(), 0, "loading leaves no asset file open");
    let mut received = Vec::new();
    for offset in (0..data.len()).step_by(MAX_CHUNK_BYTES) {
        let reply = assets
            .respond(&request("index.js", offset as u64, MAX_CHUNK_BYTES))
            .unwrap();
        assert_eq!(reply["status"], "ok");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(reply["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(bytes.len(), MAX_CHUNK_BYTES);
        assert_eq!(reply["done"], offset + MAX_CHUNK_BYTES == data.len());
        received.extend(bytes);
        assert_eq!(
            held_files(),
            0,
            "a completed request leaves no asset file open"
        );
    }
    assert_eq!(received, data);
}

//! An isolated pinned homeCore, with authenticated MQTT and owned loopback relays.
use reqwest::{Client, RequestBuilder};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command, time::Duration};
use tokio::task::JoinHandle;

const IMAGE: &str = "ghcr.io/homecore-io/hc-core@sha256:cb336921beffbb0751737cb21f8586bd3418ee10dd2c2126e8a2cd060884664f";

#[allow(dead_code)]
pub struct RegistryPackage<'a> {
    pub archive: &'a std::path::Path,
    pub id: &'a str,
    pub version: &'a str,
}

pub struct Core {
    pub root: PathBuf,
    pub mqtt_port: u16,
    name: String,
    base: String,
    token: String,
    http: Client,
    relays: Vec<JoinHandle<()>>,
}

impl Drop for Core {
    fn drop(&mut self) {
        for relay in &self.relays {
            relay.abort();
        }
        let _ = Command::new("timeout")
            .args(["20", "docker", "rm", "-f", "-v", &self.name])
            .output();
        let _ = Command::new("timeout")
            .args(["20", "docker", "network", "rm", &self.name])
            .output();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Core {
    pub async fn start() -> Self {
        Self::start_for_plugin("plugin.teslemetry-powerwall").await
    }

    pub async fn start_for_plugin(plugin: &str) -> Self {
        Self::start_fixture(Some(plugin), None).await
    }

    #[allow(dead_code)]
    pub async fn start_with_registry(package: RegistryPackage<'_>) -> Self {
        Self::start_fixture(None, Some(package)).await
    }

    async fn start_fixture(plugin: Option<&str>, package: Option<RegistryPackage<'_>>) -> Self {
        let name = format!("homecore-card-contract-{}", random_id());
        let root = std::env::temp_dir().join(&name);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let mut core = Self {
            name,
            root,
            mqtt_port: 0,
            base: String::new(),
            token: String::new(),
            http: Client::builder()
                .timeout(Duration::from_secs(7))
                .build()
                .unwrap(),
            relays: Vec::new(),
        };
        let home = core.root.join("core");
        fs::create_dir_all(home.join("config")).unwrap();
        let password = random_id();
        fs::write(core.root.join("mqtt-password"), &password).unwrap();
        let client = plugin
            .map(|id| format!("[[broker.clients]]\nid = \"{id}\"\npassword = \"{password}\"\n"))
            .unwrap_or_default();
        let registry = package
            .map(|package| write_registry(&home, package))
            .unwrap_or_default();
        fs::write(
            home.join("config/homecore.toml"),
            format!(
                r#"
[server]
host = "0.0.0.0"
port = 8080
[broker]
host = "0.0.0.0"
port = 1883
v5_port = 0
[[broker.clients]]
id = "internal.core"
password = "{password}"
{client}
{registry}
[auth]
token_expiry_hours = 1
[logging]
level = "error"
"#
            ),
        )
        .unwrap();
        docker(&["network", "create", "--internal", &core.name]);
        let mount = format!("type=bind,src={},dst=/homecore", home.display());
        docker(&[
            "run",
            "--detach",
            "--name",
            &core.name,
            "--network",
            &core.name,
            "--mount",
            &mount,
            "--security-opt",
            "no-new-privileges",
            IMAGE,
        ]);
        let ip = docker(&[
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            &core.name,
        ]);
        // Docker's internal network has no published ports or external egress.
        // These owned listeners expose only the two fixture ports on host loopback.
        let (api_port, relay) = forward(ip.clone(), 8080).await;
        core.relays.push(relay);
        let (mqtt_port, relay) = forward(ip, 1883).await;
        core.relays.push(relay);
        core.mqtt_port = mqtt_port;
        core.base = format!("http://127.0.0.1:{api_port}/api/v1");
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if core
                    .anonymous_get("/health")
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("isolated homeCore readiness");
        let initial = fs::read_to_string(home.join("INITIAL_ADMIN_PASSWORD")).unwrap();
        let password = initial
            .lines()
            .find_map(|l| l.strip_prefix("Password: "))
            .unwrap();
        let login = core
            .http
            .post(format!("{}/auth/login", core.base))
            .json(&json!({"username": "admin", "password": password}))
            .send()
            .await
            .unwrap();
        assert!(login.status().is_success(), "fixture admin login");
        let login: Value = login.json().await.unwrap();
        core.token = login["token"].as_str().expect("fixture token").to_owned();
        core
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.anonymous_get(path).bearer_auth(&self.token)
    }

    #[allow(dead_code)]
    pub fn post(&self, path: &str) -> RequestBuilder {
        self.http
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
    }

    pub fn anonymous_get(&self, path: &str) -> RequestBuilder {
        self.http.get(format!("{}{path}", self.base))
    }
}

fn write_registry(home: &std::path::Path, package: RegistryPackage<'_>) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ring::{
        digest::{digest, SHA256},
        signature::{Ed25519KeyPair, KeyPair},
    };

    let dir = home.join("fixture-registry");
    fs::create_dir(&dir).unwrap();
    let artifact = fs::read(package.archive).unwrap();
    let size = artifact.len();
    let hash: String = digest(&SHA256, &artifact)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    fs::write(dir.join("plugin.tar.zst"), artifact).unwrap();
    let index = json!({
        "schema": "1",
        "plugins": [{
            "id": package.id, "name": package.id,
            "versions": [{
                "version": package.version,
                "artifacts": [{
                    "os": "linux", "arch": "x86_64", "runtime": "native",
                    "url": "file:///homecore/fixture-registry/plugin.tar.zst",
                    "sha256": hash, "size": size
                }]
            }]
        }]
    })
    .to_string();
    let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let key = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
    fs::write(dir.join("index.json"), &index).unwrap();
    fs::write(
        dir.join("index.json.sig"),
        STANDARD.encode(key.sign(index.as_bytes())),
    )
    .unwrap();
    format!(
        "[registry]\nurl = \"file:///homecore/fixture-registry/index.json\"\npublic_key = \"{}\"\n",
        STANDARD.encode(key.public_key())
    )
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("timeout")
        .args(["20", "docker"])
        .args(args)
        .output()
        .unwrap();
    // Do not print container logs: first boot logs its generated admin password.
    assert!(
        out.status.success(),
        "isolated homeCore Docker operation failed"
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn random_id() -> String {
    let mut bytes = [0u8; 12];
    SystemRandom::new().fill(&mut bytes).unwrap();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn forward(ip: String, port: u16) -> (u16, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept(), if connections.len() < 16 => {
                    let (mut local, _) = accepted.unwrap();
                    let destination = (ip.clone(), port);
                    connections.spawn(async move {
                        if let Ok(mut remote) = tokio::net::TcpStream::connect(destination).await {
                            let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
                        }
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
    });
    (local_port, task)
}

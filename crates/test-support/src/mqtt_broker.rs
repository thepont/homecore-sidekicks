//! Owned, authenticated Mosquitto fixture; its only published port is loopback.
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Publish, QoS};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const IMAGE: &str =
    "eclipse-mosquitto@sha256:6a332cc7a8dbe011cbc076180cbb76b37c6f0f55809682ea08e7980ef0112ad9";
pub struct Broker {
    pub port: u16,
    name: String,
    root: PathBuf,
}
impl Drop for Broker {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
        for f in ["mosquitto.conf", "passwords"] {
            let _ = std::fs::remove_file(self.root.join(f));
        }
        let _ = std::fs::remove_dir(&self.root);
    }
}
impl Broker {
    pub async fn restart(&mut self) {
        assert!(std::process::Command::new("docker")
            .args(["restart", &self.name])
            .output()
            .unwrap()
            .status
            .success());
        let result = std::process::Command::new("docker")
            .args([
                "inspect",
                "-f",
                "{{(index (index .NetworkSettings.Ports \"1883/tcp\") 0).HostPort}}",
                &self.name,
            ])
            .output()
            .unwrap();
        assert!(result.status.success());
        self.port = String::from_utf8(result.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if tokio::net::TcpStream::connect(("127.0.0.1", self.port))
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("restarted broker readiness");
    }
    pub async fn start() -> Self {
        let name = format!(
            "home-hue-mqtt-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let root = std::env::temp_dir().join(&name);
        std::fs::create_dir(&root).unwrap();
        let mut owned = Self {
            port: 0,
            name,
            root,
        };
        use std::os::unix::fs::MetadataExt;
        let owner = std::fs::metadata(&owned.root).unwrap();
        let user = format!("{}:{}", owner.uid(), owner.gid());
        let mount = format!("type=bind,src={},dst=/fixture", owned.root.display());
        let result = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--user",
                &user,
                "--entrypoint",
                "mosquitto_passwd",
                "--mount",
                &mount,
                IMAGE,
                "-b",
                "-c",
                "/fixture/passwords",
                "fixture-user",
                "fixture-password",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "Mosquitto password fixture setup failed"
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            owned.root.join("passwords"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        std::fs::write(owned.root.join("mosquitto.conf"),"listener 1883\nallow_anonymous false\npassword_file /fixture/passwords\npersistence false\nlog_dest stdout\n").unwrap();
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let publish = format!(
            "127.0.0.1:{}:1883",
            reservation.local_addr().unwrap().port()
        );
        drop(reservation);
        let result = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "-d",
                "--name",
                &owned.name,
                "--user",
                "1883:1883",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--publish",
                &publish,
                "--mount",
                &format!("{mount},readonly"),
                IMAGE,
                "mosquitto",
                "-c",
                "/fixture/mosquitto.conf",
            ])
            .output()
            .unwrap();
        assert!(result.status.success(), "Mosquitto fixture start failed");
        let result = std::process::Command::new("docker")
            .args([
                "inspect",
                "-f",
                "{{(index (index .NetworkSettings.Ports \"1883/tcp\") 0).HostPort}}",
                &owned.name,
            ])
            .output()
            .unwrap();
        assert!(result.status.success());
        owned.port = String::from_utf8(result.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if tokio::net::TcpStream::connect(("127.0.0.1", owned.port))
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("Mosquitto fixture readiness");
        owned
    }
}
pub struct Probe {
    pub client: AsyncClient,
    pub messages: tokio::sync::mpsc::Receiver<Publish>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Probe {
    pub async fn start(port: u16) -> Self {
        let mut options = MqttOptions::new(
            format!("hue-probe-{}", NEXT.fetch_add(1, Ordering::SeqCst)),
            "127.0.0.1",
            port,
        );
        options.set_credentials("fixture-user", "fixture-password");
        options.set_keep_alive(Duration::from_secs(5));
        // homeCore's UI protocol returns 128 KiB file chunks encoded as base64.
        options.set_max_packet_size(256 * 1024, 256 * 1024);
        let (client, mut events) = AsyncClient::new(options, 16);
        let (tx, messages) = tokio::sync::mpsc::channel(32);
        let ready = Arc::new(tokio::sync::Notify::new());
        let signal = ready.clone();
        let task = tokio::spawn(async move {
            while let Ok(event) = events.poll().await {
                match event {
                    Event::Incoming(Incoming::SubAck(_)) => signal.notify_one(),
                    Event::Incoming(Incoming::Publish(p)) => {
                        let _ = tx.try_send(p);
                    }
                    _ => {}
                }
            }
        });
        client.subscribe("#", QoS::AtLeastOnce).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), ready.notified())
            .await
            .expect("authenticated MQTT probe subscription");
        Self {
            client,
            messages,
            task,
        }
    }
}

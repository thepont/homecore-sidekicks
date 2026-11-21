use axum::{http::HeaderMap, routing::post, Router};
use homecore_test_support::homecore;
use std::{
    process::{Child, Command},
    time::Duration,
};
use tokio::net::TcpListener;

struct Service(Child);

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
#[ignore = "requires Docker and the pinned homeCore image"]
async fn homecore_exposes_the_tv_as_a_read_only_media_player() {
    let core = homecore::Core::start_for_plugin("plugin.electrolux.living_room").await;
    async fn response(headers: HeaderMap) -> &'static str {
        if headers
            .get("soapaction")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("GetVolume")
        {
            "<r><CurrentVolume>14</CurrentVolume></r>"
        } else {
            "<r><CurrentMute>0</CurrentMute></r>"
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/upnp/control/mingusrcr", post(response)),
        )
        .await
        .unwrap()
    });
    let _service = Service(
        Command::new(env!("CARGO_BIN_EXE_homecore-hisense"))
            .env("HISENSE_IP", addr.ip().to_string())
            .env("HISENSE_PORT", addr.port().to_string())
            .env("HISENSE_POLL_SECS", "5")
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", core.mqtt_port.to_string())
            .env("MQTT_USERNAME", "plugin.electrolux.living_room")
            .env("MQTT_PASSWORD_FILE", core.root.join("mqtt-password"))
            .spawn()
            .expect("start the Hisense binary"),
    );
    let device = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Ok(response) = core.get("/devices/hisense_living_room").send().await {
                if response.status().is_success() {
                    let device: serde_json::Value = response.json().await.unwrap();
                    if device["available"] == true && device["attributes"]["volume"] == 14 {
                        break device;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("registered TV device");
    assert_eq!(device["device_type"], "media_player");
    assert_eq!(device["attributes"]["muted"], false);
    let schema = core
        .get("/devices/hisense_living_room/schema")
        .send()
        .await
        .unwrap();
    assert!(schema.status().is_success(), "TV schema must be registered");
    let schema: serde_json::Value = schema.json().await.unwrap();
    assert_eq!(schema["attributes"]["volume"]["writable"], false);
    assert_eq!(schema["attributes"]["muted"]["writable"], false);
    server.abort();
}

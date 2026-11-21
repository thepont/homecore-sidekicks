#[allow(dead_code)]
#[path = "../../electrolux/tests/support/homecore.rs"]
mod homecore;
use axum::{http::HeaderMap, routing::post, Router};
use homecore_hisense::{run, Broker, Hisense};
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
#[ignore = "requires Docker and the pinned homeCore image"]
async fn homecore_exposes_the_tv_as_a_read_only_media_player() {
    let core = homecore::Core::start().await;
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
    let password = std::fs::read_to_string(core.root.join("mqtt-password")).unwrap();
    let task = tokio::spawn(run(
        Hisense::new(addr, Duration::from_secs(1)).unwrap(),
        Broker {
            host: "127.0.0.1".into(),
            port: core.mqtt_port,
            username: "plugin.electrolux.living_room".into(),
            password,
        },
        Duration::from_millis(100),
    ));
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
    task.abort();
    server.abort();
}

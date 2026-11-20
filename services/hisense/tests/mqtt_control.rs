#[allow(dead_code)]
#[path = "../../electrolux/tests/support/mqtt_broker.rs"]
mod mqtt_broker;
use axum::{extract::State, http::HeaderMap, routing::post, Router};
use homecore_hisense::{run, Broker, Hisense};
use rumqttc::QoS;
use std::time::Duration;
use tokio::{net::TcpListener, sync::mpsc};

#[tokio::test]
#[ignore = "requires Docker and the pinned Mosquitto image"]
async fn accepts_live_valid_commands_and_discards_retained_or_invalid_messages() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let topic = "homecore/devices/hisense_living_room/cmd";
    probe
        .client
        .publish(topic, QoS::AtLeastOnce, true, r#"{"muted":true}"#)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let p = probe.messages.recv().await.unwrap();
            if p.topic == topic && p.payload.as_ref() == br#"{"muted":true}"# {
                break;
            }
        }
    })
    .await
    .expect("retained message stored");
    let (tx, mut rx) = mpsc::channel::<String>(8);
    async fn response(
        State(tx): State<mpsc::Sender<String>>,
        headers: HeaderMap,
        body: String,
    ) -> &'static str {
        let action = headers.get("soapaction").unwrap().to_str().unwrap();
        if action.contains("#Set") {
            tx.send(body).await.unwrap();
            return "<r/>";
        }
        if action.contains("#GetVolume") {
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
            Router::new()
                .route("/upnp/control/mingusrcr", post(response))
                .with_state(tx),
        )
        .await
        .unwrap()
    });
    let task = tokio::spawn(run(
        Hisense::new(addr, Duration::from_secs(1)).unwrap(),
        Broker {
            host: "127.0.0.1".into(),
            port: broker.port,
            username: "fixture-user".into(),
            password: "fixture-password".into(),
        },
        Duration::from_millis(100),
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let p = probe.messages.recv().await.unwrap();
            if p.topic.ends_with("/availability") && p.payload.as_ref() == b"online" {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "retained command must be ignored on connect"
    );
    probe
        .client
        .publish(topic, QoS::AtLeastOnce, true, "")
        .await
        .unwrap();
    probe
        .client
        .publish(topic, QoS::AtLeastOnce, false, r#"{"volume":23}"#)
        .await
        .unwrap();
    let body = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(body.contains("<DesiredVolume>23</DesiredVolume>"));
    probe
        .client
        .publish(topic, QoS::AtLeastOnce, false, r#"{"volume":101}"#)
        .await
        .unwrap();
    assert!(tokio::time::timeout(Duration::from_millis(300), rx.recv())
        .await
        .is_err());
    probe
        .client
        .publish(topic, QoS::AtLeastOnce, false, r#"{"muted":true}"#)
        .await
        .unwrap();
    let body = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(body.contains("<DesiredMute>1</DesiredMute>"));
    task.abort();
    server.abort();
}

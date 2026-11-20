use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use homecore_hisense::{Control, Hisense};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

type Requests = Arc<Mutex<Vec<(String, String)>>>;

#[tokio::test]
async fn sends_only_validated_volume_and_mute_commands() {
    let requests = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    async fn receive(State(out): State<Requests>, headers: HeaderMap, body: String) -> StatusCode {
        out.lock().unwrap().push((
            headers
                .get("soapaction")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            body,
        ));
        StatusCode::OK
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let captured = requests.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/upnp/control/mingusrcr", post(receive))
                .with_state(captured),
        )
        .await
        .unwrap()
    });
    let tv = Hisense::new(address, Duration::from_secs(1)).unwrap();
    tv.control(Control::Volume(23)).await.unwrap();
    tv.control(Control::Mute(true)).await.unwrap();
    let got = requests.lock().unwrap();
    assert_eq!(got.len(), 2);
    assert!(got[0].0.contains("#SetVolume"));
    assert!(got[0].1.contains("<DesiredVolume>23</DesiredVolume>"));
    assert!(got[1].0.contains("#SetMute"));
    assert!(got[1].1.contains("<DesiredMute>1</DesiredMute>"));
    task.abort();
}

#[test]
fn rejects_ambiguous_unknown_and_invalid_commands() {
    for command in [
        r#"{"volume":101}"#,
        r#"{"volume":-1}"#,
        r#"{"volume":1.5}"#,
        r#"{"muted":"true"}"#,
        r#"{"volume":7,"muted":true}"#,
        r#"{"volume":7,"volume":8}"#,
        r#"{"unknown":true}"#,
        r#"{}"#,
    ] {
        assert!(Control::parse(command.as_bytes()).is_err(), "{command}");
    }
    assert_eq!(
        Control::parse(br#"{"volume":0,"correlation_id":"x","_hc":{}}"#).unwrap(),
        Control::Volume(0)
    );
    assert_eq!(
        Control::parse(br#"{"muted":false}"#).unwrap(),
        Control::Mute(false)
    );
}

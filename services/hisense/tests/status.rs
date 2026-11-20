use axum::{extract::State, http::HeaderMap, routing::post, Router};
use homecore_hisense::{Hisense, Reading};
use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpListener;

async fn server(volume: &'static str, mute: &'static str) -> SocketAddr {
    async fn response(
        State((volume, mute)): State<(&'static str, &'static str)>,
        headers: HeaderMap,
    ) -> String {
        let action = headers
            .get("soapaction")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if action.contains("#GetVolume") {
            volume.into()
        } else if action.contains("#GetMute") {
            mute.into()
        } else {
            String::new()
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/upnp/control/mingusrcr", post(response))
        .with_state((volume, mute));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    address
}

#[tokio::test]
async fn reads_volume_and_mute_from_device() {
    let a = server(
        "<Envelope><Body><CurrentVolume>14</CurrentVolume></Body></Envelope>",
        "<Envelope><Body><CurrentMute>0</CurrentMute></Body></Envelope>",
    )
    .await;
    let tv = Hisense::new(a, Duration::from_secs(2)).unwrap();
    assert_eq!(
        tv.read().await.unwrap(),
        Reading {
            volume: 14,
            muted: false
        }
    );
}

#[tokio::test]
async fn rejects_invalid_device_readings() {
    let a = server(
        "<Envelope><CurrentVolume>101</CurrentVolume></Envelope>",
        "<Envelope><CurrentMute>0</CurrentMute></Envelope>",
    )
    .await;
    assert!(Hisense::new(a, Duration::from_secs(2))
        .unwrap()
        .read()
        .await
        .is_err());
    let a = server(
        "<Envelope><CurrentVolume>14</CurrentVolume></Envelope>",
        "<Envelope><CurrentMute>maybe</CurrentMute></Envelope>",
    )
    .await;
    assert!(Hisense::new(a, Duration::from_secs(2))
        .unwrap()
        .read()
        .await
        .is_err());
}

#[tokio::test]
async fn rejects_ambiguous_and_malformed_xml() {
    for volume in [
        "<r><CurrentVolume>10</CurrentVolume><CurrentVolume>11</CurrentVolume></r>",
        "<r><CurrentVolume>14</CurrentVolume>",
        "<!DOCTYPE r><r><CurrentVolume>14</CurrentVolume></r>",
    ] {
        let a = server(volume, "<r><CurrentMute>1</CurrentMute></r>").await;
        assert!(Hisense::new(a, Duration::from_secs(2))
            .unwrap()
            .read()
            .await
            .is_err());
    }
}

#[tokio::test]
#[ignore = "requires the real TV address in HISENSE_LIVE_ADDRESS"]
async fn reads_the_real_tv_without_sending_commands() {
    let address = std::env::var("HISENSE_LIVE_ADDRESS")
        .unwrap()
        .parse()
        .unwrap();
    let reading = Hisense::new(address, Duration::from_secs(3))
        .unwrap()
        .read()
        .await
        .unwrap();
    assert!(reading.volume <= 100);
    println!(
        "TV reports volume {}%, muted {}",
        reading.volume, reading.muted
    );
}

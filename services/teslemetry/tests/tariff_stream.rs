mod support;

use axum::{
    response::{sse::Event, Sse},
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use futures_util::stream;
use homecore_teslemetry::totals::{self, TariffState};
use serde_json::{json, Map, Value};
use std::{convert::Infallible, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

struct Tasks(Vec<JoinHandle<()>>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

fn tariff(site: u64, observed: DateTime<Utc>, buy: f64, sell: f64) -> Value {
    json!({
        "site_id": site, "createdAt": observed.to_rfc3339(), "isCache": true,
        "tariff_content_v2": {
            "energy_charges": {"ALL": {"rates": {"ALL": buy}}},
            "sell_tariff": {"energy_charges": {"ALL": {"rates": {"ALL": sell}}}}
        }
    })
}

async fn send(frames: &mpsc::Sender<Event>, kind: &str, value: Value) {
    frames
        .send(Event::default().event(kind).data(value.to_string()))
        .await
        .unwrap();
}

/// An accepted energy event proves that all earlier frames on this stream ran.
async fn barrier(
    frames: &mpsc::Sender<Event>,
    totals: &mut watch::Receiver<Map<String, Value>>,
    observed: DateTime<Utc>,
    marker: u32,
) {
    send(
        frames,
        "energy_totals",
        json!({
            "id": 42, "date": observed.format("%Y-%m-%d").to_string(),
            "createdAt": (observed + chrono::Duration::seconds(i64::from(marker))).to_rfc3339(),
            "totals": {"total_home_usage": marker}
        }),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if totals
                .borrow_and_update()
                .get("total_home_usage_wh")
                .and_then(Value::as_f64)
                == Some(f64::from(marker))
            {
                return;
            }
            totals.changed().await.unwrap();
        }
    })
    .await
    .expect("stream must process the barrier event");
}

fn assert_prices(state: &TariffState, at: DateTime<Utc>, buy: f64, sell: f64) {
    let fields = state.readings(at);
    assert_eq!(fields["tariff_available"], true);
    assert_eq!(fields["tariff_buy_price_per_kwh"], buy);
    assert_eq!(fields["tariff_sell_price_per_kwh"], sell);
    assert_eq!(fields["tariff_currency"], "unspecified");
}

fn assert_unavailable(state: &TariffState, at: DateTime<Utc>) {
    let fields = state.readings(at);
    assert_eq!(fields["tariff_available"], false);
    assert!(!fields.contains_key("tariff_buy_price_per_kwh"));
    assert!(!fields.contains_key("tariff_sell_price_per_kwh"));
}

async fn exercise_tariff_stream_lifecycle() {
    let (connected, mut connections) = mpsc::unbounded_channel();
    let app = Router::new().route(
        "/sse",
        get(move || {
            let connected = connected.clone();
            async move {
                let (frames, receiver) = mpsc::channel(16);
                connected.send(frames).unwrap();
                Sse::new(stream::unfold(receiver, |mut receiver| async move {
                    receiver
                        .recv()
                        .await
                        .map(|event| (Ok::<_, Infallible>(event), receiver))
                }))
            }
        }),
    );
    let (url, server) = support::serve(app).await;
    let (totals_tx, mut totals_rx) = watch::channel(Map::new());
    let (tariffs_tx, mut tariffs_rx) = watch::channel(TariffState::default());
    let listener = tokio::spawn(totals::listen(
        url,
        42,
        "fixture-token".into(),
        totals_tx,
        tariffs_tx,
    ));
    let _tasks = Tasks(vec![server, listener]);
    let frames = tokio::time::timeout(Duration::from_secs(2), connections.recv())
        .await
        .unwrap()
        .unwrap();
    let now: DateTime<Utc> = std::time::SystemTime::now().into();
    // Old configuration is legitimate: tariff events only change when the plan changes.
    let observed = now - chrono::Duration::minutes(10);
    send(
        &frames,
        "tariff_content_v2",
        tariff(42, observed, 0.2715, 0.0421),
    )
    .await;
    barrier(&frames, &mut totals_rx, observed, 1).await;
    assert_unavailable(&tariffs_rx.borrow(), now);

    let site_info = json!({"site_id":42,"createdAt":observed.to_rfc3339(),"site_info":{"installation_time_zone":"Australia/Melbourne"}});
    send(&frames, "site_info", site_info.clone()).await;
    barrier(&frames, &mut totals_rx, observed, 2).await;
    assert_prices(&tariffs_rx.borrow(), now, 0.2715, 0.0421);

    send(
        &frames,
        "tariff_content_v2",
        tariff(99, observed + chrono::Duration::seconds(1), 9.91, 8.81),
    )
    .await;
    send(
        &frames,
        "tariff_content_v2",
        tariff(42, observed - chrono::Duration::seconds(1), 7.71, 6.61),
    )
    .await;
    send(
        &frames,
        "tariff_content_v2",
        tariff(42, now + chrono::Duration::days(1), 5.51, 4.41),
    )
    .await;
    barrier(&frames, &mut totals_rx, observed, 3).await;
    assert_prices(&tariffs_rx.borrow(), now, 0.2715, 0.0421);

    send(&frames, "tariff_content_v2", json!({"site_id":42,"createdAt":(observed + chrono::Duration::seconds(2)).to_rfc3339(),"tariff_content_v2":null})).await;
    barrier(&frames, &mut totals_rx, observed, 4).await;
    assert_unavailable(&tariffs_rx.borrow(), now);

    send(
        &frames,
        "tariff_content_v2",
        tariff(42, observed + chrono::Duration::seconds(3), 0.3197, 0.0873),
    )
    .await;
    barrier(&frames, &mut totals_rx, observed, 5).await;
    assert_prices(&tariffs_rx.borrow(), now, 0.3197, 0.0873);
    send(&frames, "tariff_content_v2", json!({"site_id":42,"createdAt":(observed + chrono::Duration::seconds(4)).to_rfc3339(),"tariff_content_v2":{"energy_charges":"invalid"}})).await;
    barrier(&frames, &mut totals_rx, observed, 6).await;
    assert_unavailable(&tariffs_rx.borrow(), now);

    let cached = tariff(42, observed + chrono::Duration::seconds(5), 0.1981, -0.0123);
    send(&frames, "tariff_content_v2", cached.clone()).await;
    barrier(&frames, &mut totals_rx, observed, 7).await;
    assert_prices(&tariffs_rx.borrow(), now, 0.1981, -0.0123);
    drop(frames);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if tariffs_rx
                .borrow_and_update()
                .readings(now)
                .get("tariff_available")
                == Some(&Value::Bool(false))
            {
                return;
            }
            tariffs_rx.changed().await.unwrap();
        }
    })
    .await
    .expect("disconnect must withdraw current prices");
    assert_unavailable(&tariffs_rx.borrow(), now);

    let frames = tokio::time::timeout(Duration::from_secs(3), connections.recv())
        .await
        .unwrap()
        .unwrap();
    send(&frames, "site_info", site_info).await;
    send(&frames, "tariff_content_v2", cached).await;
    barrier(&frames, &mut totals_rx, observed, 8).await;
    assert_prices(&tariffs_rx.borrow(), now, 0.1981, -0.0123);
}

#[tokio::test]
async fn tariff_stream_scopes_orders_withdraws_and_recovers_cached_prices() {
    tokio::time::timeout(Duration::from_secs(8), exercise_tariff_stream_lifecycle())
        .await
        .expect("tariff stream lifecycle must complete within its deadline");
}

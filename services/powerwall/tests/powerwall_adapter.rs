#[allow(dead_code)]
#[path = "support/mqtt_broker.rs"]
mod mqtt_broker;
mod support;
use axum::{
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct OwnedChild(Child, PathBuf);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_file(self.1.join("token"));
        let _ = std::fs::remove_file(self.1.join("mqtt-password"));
        let _ = std::fs::remove_dir(&self.1);
    }
}

fn start_adapter(url: &str, port: u16, name: &str, interval: Option<&str>) -> OwnedChild {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("token"), "fixture-token").unwrap();
    std::fs::write(root.join("mqtt-password"), "fixture-password").unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_powerwall-bridge"));
    cmd.env("TESLEMETRY_URL", url)
        .env("TESLEMETRY_SITE_ID", "42")
        .env("TESLEMETRY_TOKEN_FILE", root.join("token"))
        .env("MQTT_HOST", "127.0.0.1")
        .env("MQTT_PORT", port.to_string())
        .env("MQTT_USERNAME", "fixture-user")
        .env("MQTT_PASSWORD_FILE", root.join("mqtt-password"))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(interval) = interval {
        cmd.env("POLL_INTERVAL_SECS", interval);
    }
    OwnedChild(cmd.spawn().unwrap(), root)
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_powerwall_reading_when_polled_then_homecore_gets_read_only_state() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let app = Router::new().route(
        "/api/1/energy_sites/42/live_status",
        get(|headers: HeaderMap| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok())
                != Some("Bearer fixture-token")
            {
                return (StatusCode::UNAUTHORIZED, Json(json!({})));
            }
            (
                StatusCode::OK,
                Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})),
            )
        }),
    );
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "powerwall-fixture", None);
    let mut state = None;
    let got = tokio::time::timeout(Duration::from_secs(4), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                state = Some(serde_json::from_slice::<Value>(&message.payload).unwrap());
                return;
            }
        }
    })
    .await;
    assert!(got.is_ok(), "battery state must arrive at the broker");
    assert_eq!(
        state,
        Some(json!({"charge_pct":65.2,"battery_power_w":-1190.0}))
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_source_failure_after_success_when_polled_then_homecore_marks_it_offline() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route(
        "/api/1/energy_sites/42/live_status",
        get(move || {
            let calls = calls.clone();
            async move {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    (
                        StatusCode::OK,
                        Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})),
                    )
                } else {
                    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({})))
                }
            }
        }),
    );
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "powerwall-failure-fixture", Some("1"));
    let mut saw_online = false;
    let done = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/availability" {
                if message.payload.as_ref() == b"online" {
                    saw_online = true;
                }
                if saw_online && message.payload.as_ref() == b"offline" {
                    return;
                }
            }
        }
    })
    .await;
    assert!(
        done.is_ok(),
        "source failure must mark an online Powerwall offline"
    );
    server.abort();
}

#[tokio::test]
async fn oversized_unfinished_credential_is_rejected_without_waiting_for_eof() {
    use std::io::Write;
    let root =
        std::env::temp_dir().join(format!("powerwall-secret-fixture-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    assert!(Command::new("mkfifo")
        .arg(root.join("token"))
        .status()
        .unwrap()
        .success());
    let child = Command::new(env!("CARGO_BIN_EXE_powerwall-bridge"))
        .env("TESLEMETRY_URL", "https://api.teslemetry.com")
        .env("TESLEMETRY_SITE_ID", "42")
        .env("TESLEMETRY_TOKEN_FILE", root.join("token"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut owned = OwnedChild(child, root.clone());
    let _writer = std::thread::spawn(move || {
        let mut fifo = std::fs::OpenOptions::new()
            .write(true)
            .open(root.join("token"))
            .unwrap();
        fifo.write_all(&vec![b'x'; 4097]).unwrap();
        std::thread::sleep(Duration::from_secs(4));
    });
    let finished = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(status) = owned.0.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        finished.is_ok(),
        "invalid credential should be rejected before EOF"
    );
    assert!(
        !finished.unwrap().success(),
        "invalid credential must fail closed"
    );
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_energy_site_when_polled_then_homecore_receives_energy_and_charger_readings() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let app = Router::new().route(
        "/api/1/energy_sites/42/live_status",
        get(|| async {
            Json(json!({"response": {
                "timestamp": chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
                "percentage_charged": 65.2, "battery_power": -1190,
                "solar_power": 2412, "grid_power": -340, "load_power": 882,
                "generator_power": 0, "grid_status": "Active", "island_status": "OnGrid",
                "storm_mode_active": false,
                "wall_connectors": [{"din":"fixture-charger", "wall_connector_power": 3120,
                    "wall_connector_state": 2, "wall_connector_fault_state": 0,
                    "ocpp_status": 1, "powershare_session_state": 0}]
            }}))
        }),
    );
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "energy-site-fixture", None);
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                return serde_json::from_slice::<Value>(&message.payload).unwrap();
            }
        }
        panic!("energy state stream closed");
    })
    .await
    .expect("energy state must arrive");
    assert_eq!(state["solar_power_w"], 2412.0);
    assert_eq!(state["grid_power_w"], -340.0);
    assert_eq!(state["home_power_w"], 882.0);
    assert_eq!(state["generator_power_w"], 0.0);
    assert_eq!(state["grid_status"], "Active");
    assert_eq!(state["island_status"], "OnGrid");
    assert_eq!(state["storm_mode_active"], false);
    assert_eq!(state["wall_connector_1_power_w"], 3120.0);
    assert_eq!(state["wall_connector_1_state"], 2);
    assert_eq!(state["wall_connector_1_fault_state"], 0);
    assert_eq!(state["wall_connector_1_ocpp_status"], 1);
    assert_eq!(state["wall_connector_1_powershare_state"], 0);
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_energy_site_when_polled_then_homecore_gets_read_only_sensor_schema() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let app = Router::new().route("/api/1/energy_sites/42/live_status", get(|| async {
        Json(json!({"response": {"timestamp": chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
            "percentage_charged": 65.2, "battery_power": -1190, "solar_power": 2412,
            "grid_power": -340, "load_power": 882, "generator_power": 0,
            "grid_status": "Active", "island_status": "OnGrid", "storm_mode_active": false,
            "wall_connectors": [{"wall_connector_power": 3120, "wall_connector_state": 2}]}}))
    }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "energy-schema-fixture", None);
    let schema = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/schema" {
                let value = serde_json::from_slice::<Value>(&message.payload).unwrap();
                if value["attributes"]["solar_power_w"].is_object() {
                    return value;
                }
            }
        }
        panic!("schema stream closed");
    })
    .await
    .expect("energy sensor schema must arrive");
    for field in [
        "solar_power_w",
        "grid_power_w",
        "home_power_w",
        "wall_connector_1_power_w",
    ] {
        assert_eq!(schema["attributes"][field]["unit"], "W");
        assert_eq!(schema["attributes"][field]["writable"], false);
    }
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_energy_totals_stream_when_polled_then_homecore_gets_dated_energy_sensors() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = format!(
        "event: energy_totals\ndata: {}\n\n",
        json!({"id":"42","date":now.format("%Y-%m-%d").to_string(),"createdAt":now.to_rfc3339(),"isCache":true,"totals":{"total_solar_generation":12500,"total_home_usage":6800,"total_battery_charge":2100,"total_battery_discharge":400,"grid_energy_imported":120,"total_grid_energy_exported":450}})
    );
    let app = Router::new()
        .route("/sse", get(move || { let event=event.clone(); async move { ([(("content-type"), "text/event-stream")], event) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async {
            Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}}))
        }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "energy-totals-fixture", Some("1"));
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                let state = serde_json::from_slice::<Value>(&message.payload).unwrap();
                if state["total_solar_generation_wh"].is_number() {
                    return state;
                }
            }
        }
        panic!("state stream closed")
    })
    .await
    .expect("dated energy totals must arrive");
    assert_eq!(state["total_solar_generation_wh"], 12500.0);
    assert_eq!(state["total_home_usage_wh"], 6800.0);
    assert_eq!(state["grid_energy_imported_wh"], 120.0);
    assert!(state["energy_totals_date"].is_string());
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_older_totals_replayed_after_newer_event_then_homecore_keeps_newer_reading() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = |time: chrono::DateTime<chrono::Utc>, value| {
        format!(
            "event: energy_totals\ndata: {}\n\n",
            json!({"id":"42","date":now.format("%Y-%m-%d").to_string(),"createdAt":time.to_rfc3339(),"totals":{"total_solar_generation":value}})
        )
    };
    let stream = format!(
        "{}{}",
        event(now, 12500),
        event(now - chrono::Duration::minutes(10), 1000)
    );
    let app = Router::new().route("/sse", get(move || { let stream=stream.clone(); async move { ([("content-type", "text/event-stream")], stream) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "energy-replay-fixture", Some("1"));
    let value = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                let state = serde_json::from_slice::<Value>(&message.payload).unwrap();
                if state["total_solar_generation_wh"].is_number() {
                    return state["total_solar_generation_wh"].clone();
                }
            }
        }
        panic!("state stream closed")
    })
    .await
    .expect("energy totals must arrive");
    assert_eq!(value, 12500.0);
    server.abort();
}

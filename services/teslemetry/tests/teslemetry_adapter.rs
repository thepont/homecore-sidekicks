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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teslemetry-bridge"));
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
    let child = Command::new(env!("CARGO_BIN_EXE_teslemetry-bridge"))
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

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_wall_connector_totals_then_homecore_receives_charged_energy_without_identifiers() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = format!(
        "event: wall_connector_totals\ndata: {}\n\n",
        json!({"id":"42","date":now.format("%Y-%m-%d").to_string(),"createdAt":now.to_rfc3339(),"totals":{"private-fixture-din":5400}})
    );
    let app = Router::new().route("/sse", get(move || { let event=event.clone(); async move { ([("content-type", "text/event-stream")], event) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190,"wall_connectors":[{"wall_connector_power":0}]}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "charger-totals-fixture", Some("1"));
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                let value = serde_json::from_slice::<Value>(&message.payload).unwrap();
                if value["wall_connectors_energy_wh"].is_number() {
                    return value;
                }
            }
        }
        panic!("state stream closed")
    })
    .await
    .expect("wall connector total must arrive");
    assert_eq!(state["wall_connectors_energy_wh"], 5400.0);
    assert!(!state.to_string().contains("private-fixture-din"));
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_site_configuration_stream_then_homecore_receives_read_only_site_sensors() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = format!(
        "event: site_info\ndata: {}\n\n",
        json!({"site_id":"42","createdAt":now.to_rfc3339(),"site_info":{"backup_reserve_percent":20.0,"default_real_mode":"self_consumption","nameplate_energy":13500.0,"nameplate_power":5000.0,"battery_count":1,"vpp_backup_reserve_percent":35.0,"version":"fixture-version","user_settings":{"storm_mode_enabled":true},"components":{"solar":true,"battery":true,"grid_services_enabled":false}}})
    );
    let app = Router::new().route("/sse", get(move || {let event=event.clone(); async move { ([("content-type", "text/event-stream")],event) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "site-config-fixture", Some("1"));
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                let value = serde_json::from_slice::<Value>(&message.payload).unwrap();
                if value["site_backup_reserve_pct"].is_number() {
                    return value;
                }
            }
        }
        panic!("state stream closed")
    })
    .await
    .expect("site config must arrive");
    assert_eq!(state["site_backup_reserve_pct"], 20.0);
    assert_eq!(state["site_operation_mode"], "self_consumption");
    assert_eq!(state["site_nameplate_energy_wh"], 13500.0);
    assert_eq!(state["site_nameplate_power_w"], 5000.0);
    assert_eq!(state["site_storm_watch_enabled"], true);
    assert_eq!(state["site_battery_count"], 1);
    assert_eq!(state["site_grid_services_enabled"], false);
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_discovered_vehicle_when_telemetry_arrives_then_homecore_receives_read_only_sensors()
{
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let wakes = Arc::new(AtomicUsize::new(0));
    let wake_count = wakes.clone();
    let event = format!(
        "event: data\ndata: {}\n\n",
        json!({"vin":vin,"createdAt":now.to_rfc3339(),"data":{"BatteryLevel":62,"Locked":true,"HvacPower":false,"InsideTemp":21.5,"Location":{"latitude":-37.8,"longitude":144.9}}})
    );
    let app = Router::new()
        .route("/api/1/vehicles", get(move || async move { Json(json!({"response":[{"vin":vin,"display_name":"Test vehicle","state":"online"}]})) }))
        .route("/api/1/vehicles/{vin}/wake_up", axum::routing::post(move || {let count=wake_count.clone(); async move {count.fetch_add(1, Ordering::SeqCst); StatusCode::OK}}))
        .route("/sse/{vin}", get(move || {let event=event.clone(); async move { ([("content-type", "text/event-stream")], event) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "vehicle-fixture", Some("1"));
    let state = tokio::time::timeout(Duration::from_secs(6), async {
        while let Some(message) = probe.messages.recv().await {
            if message
                .topic
                .starts_with("homecore/devices/teslemetry_vehicle_")
                && message.topic.ends_with("/state")
            {
                return serde_json::from_slice::<Value>(&message.payload).unwrap();
            }
        }
        panic!("vehicle state stream closed")
    })
    .await
    .expect("vehicle readings must be published");
    assert_eq!(state["battery_level"], 62);
    assert_eq!(state["locked"], true);
    assert_eq!(state["hvac_power"], false);
    assert_eq!(state["inside_temp"], 21.5);
    assert_eq!(state["location_latitude"], -37.8);
    assert_eq!(state["location_longitude"], 144.9);
    assert!(!state.to_string().contains(vin));
    assert_eq!(wakes.load(Ordering::SeqCst), 0, "vehicle must not be woken");
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_no_vehicle_in_account_then_no_vehicle_device_is_registered() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let discovered = std::sync::Arc::new(tokio::sync::Notify::new());
    let signal = discovered.clone();
    let app = Router::new().route("/api/1/vehicles", get(move || {let signal=signal.clone(); async move {signal.notify_one(); Json(json!({"response":[]}))}}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "no-vehicle-fixture", Some("1"));
    tokio::time::timeout(Duration::from_secs(5), discovered.notified())
        .await
        .expect("vehicle list must be checked");
    let unexpected = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/plugins/plugin.teslemetry-powerwall/register" {
                let item: Value = serde_json::from_slice(&message.payload).unwrap();
                if item["device_id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("teslemetry_vehicle_"))
                {
                    return true;
                }
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        !unexpected,
        "an account without vehicles must not create a vehicle card"
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_no_wall_connector_then_charger_total_is_not_shown() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = format!(
        "event: wall_connector_totals\ndata: {}\n\n",
        json!({"id":"42","date":now.format("%Y-%m-%d").to_string(),"createdAt":now.to_rfc3339(),"totals":{}})
    );
    let app = Router::new().route("/sse", get(move || {let event=event.clone(); async move { ([("content-type", "text/event-stream")],event) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190,"wall_connectors":[]}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "no-charger-fixture", Some("1"));
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut states = 0;
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                states += 1;
                if states == 2 {
                    return serde_json::from_slice::<Value>(&message.payload).unwrap();
                }
            }
        }
        panic!("state stream closed")
    })
    .await
    .expect("two readings must arrive");
    assert!(
        result.get("wall_connectors_energy_wh").is_none(),
        "no charger must not produce a charger sensor"
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_newer_vehicle_status_then_older_snapshot_fills_missing_sensors() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let stream = format!(
        "event: state\ndata: {}\n\nevent: data\ndata: {}\n\n",
        json!({"vin":vin,"createdAt":now.to_rfc3339(),"state":"asleep"}),
        json!({"vin":vin,"createdAt":(now-chrono::Duration::hours(1)).to_rfc3339(),"data":{"BatteryLevel":62}})
    );
    let app = Router::new().route("/api/1/vehicles", get(move || async move { Json(json!({"response":[{"vin":vin}]})) }))
        .route("/sse/{vin}", get(move || {let stream=stream.clone(); async move { ([("content-type", "text/event-stream")], stream) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "vehicle-cache-fixture", Some("1"));
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message
                .topic
                .starts_with("homecore/devices/teslemetry_vehicle_")
                && message.topic.ends_with("/state")
            {
                let state: Value = serde_json::from_slice(&message.payload).unwrap();
                if state["battery_level"].is_number() {
                    return state;
                }
            }
        }
        panic!("vehicle state stream closed")
    })
    .await
    .expect("cached battery should fill an unknown value");
    assert_eq!(state["battery_level"], 62);
    assert_eq!(state["vehicle_state"], "asleep");
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_vehicle_online_when_adapter_dies_then_vehicle_becomes_unavailable() {
    use futures_util::StreamExt;
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let event = format!(
        "event: data\ndata: {}\n\n",
        json!({"vin":vin,"createdAt":now.to_rfc3339(),"data":{"BatteryLevel":62}})
    );
    let app = Router::new().route("/api/1/vehicles", get(move || async move { Json(json!({"response":[{"vin":vin}]})) }))
        .route("/sse/{vin}", get(move || {let event=event.clone(); async move {
            let stream = futures_util::stream::once(async move { Ok::<_,std::convert::Infallible>(event) }).chain(futures_util::stream::pending());
            ([("content-type", "text/event-stream")], axum::body::Body::from_stream(stream))
        }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let mut owned = start_adapter(&url, broker.port, "vehicle-will-fixture", Some("1"));
    let online = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message
                .topic
                .starts_with("homecore/devices/teslemetry_vehicle_")
                && message.topic.ends_with("/availability")
                && message.payload.as_ref() == b"online"
            {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(online, Ok(true), "vehicle must first be online");
    owned.0.kill().unwrap();
    owned.0.wait().unwrap();
    let offline = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message
                .topic
                .starts_with("homecore/devices/teslemetry_vehicle_")
                && message.topic.ends_with("/availability")
                && message.payload.as_ref() == b"offline"
            {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(
        offline,
        Ok(true),
        "vehicle must become offline after an unexpected exit"
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_vehicle_reading_is_withdrawn_then_old_sensor_value_disappears() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let stream = format!(
        "event: data\ndata: {}\n\nevent: data\ndata: {}\n\n",
        json!({"vin":vin,"createdAt":now.to_rfc3339(),"data":{"BatteryLevel":62,"Locked":true}}),
        json!({"vin":vin,"createdAt":(now+chrono::Duration::seconds(1)).to_rfc3339(),"data":{"BatteryLevel":null}})
    );
    let app = Router::new().route("/api/1/vehicles", get(move || async move { Json(json!({"response":[{"vin":vin}]})) }))
        .route("/sse/{vin}", get(move || {let stream=stream.clone(); async move { ([("content-type", "text/event-stream")], stream) }}))
        .route("/api/1/energy_sites/42/live_status", get(|| async { Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}})) }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "vehicle-withdraw-fixture", Some("1"));
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        let mut saw_battery = false;
        while let Some(message) = probe.messages.recv().await {
            if message
                .topic
                .starts_with("homecore/devices/teslemetry_vehicle_")
                && message.topic.ends_with("/state")
            {
                let state: Value = serde_json::from_slice(&message.payload).unwrap();
                if state["battery_level"] == 62 {
                    saw_battery = true;
                }
                if saw_battery && state.get("battery_level").is_none() && state["locked"] == true {
                    return true;
                }
            }
        }
        false
    })
    .await;
    assert_eq!(
        outcome,
        Ok(true),
        "withdrawn vehicle value should be removed"
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_vehicle_telemetry_then_schema_describes_read_only_values() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let event = format!(
        "event: data\ndata: {}\n\n",
        json!({"vin":vin,"createdAt":now.to_rfc3339(),"data":{"BatteryLevel":62,"Locked":true,"InsideTemp":21.5}})
    );
    let app = Router::new().route("/api/1/vehicles", get(move || async move {Json(json!({"response":[{"vin":vin}]}))}))
        .route("/sse/{vin}", get(move || {let event=event.clone(); async move {([("content-type", "text/event-stream")],event)}}))
        .route("/api/1/energy_sites/42/live_status", get(|| async {Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}}))}));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "vehicle-schema-fixture", Some("1"));
    let schema = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message
                .topic
                .starts_with("homecore/devices/teslemetry_vehicle_")
                && message.topic.ends_with("/schema")
            {
                return serde_json::from_slice::<Value>(&message.payload).unwrap();
            }
        }
        panic!("schema stream closed")
    })
    .await
    .expect("vehicle schema must arrive");
    assert_eq!(schema["attributes"]["battery_level"]["unit"], "%");
    assert_eq!(schema["attributes"]["inside_temp"]["unit"], "°C");
    for field in ["battery_level", "locked", "inside_temp"] {
        assert_eq!(schema["attributes"][field]["writable"], false);
    }
    assert_eq!(schema["primary"], json!(["battery_level", "vehicle_state"]));
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_repeated_energy_polls_then_http_connection_is_reused() {
    use axum::extract::ConnectInfo;
    use std::{collections::HashSet, net::SocketAddr};
    let broker = mqtt_broker::Broker::start().await;
    let (observed, mut requests) = tokio::sync::mpsc::channel(8);
    let app = Router::new().route("/api/1/energy_sites/42/live_status", get(move |ConnectInfo(peer): ConnectInfo<SocketAddr>| {
        let observed = observed.clone();
        async move {
            observed.send(peer).await.unwrap();
            Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let _owned = start_adapter(&url, broker.port, "energy-reuse-fixture", Some("1"));
    let peers = tokio::time::timeout(Duration::from_secs(6), async {
        let mut peers = HashSet::new();
        for _ in 0..3 {
            peers.insert(requests.recv().await.unwrap());
        }
        peers
    })
    .await
    .expect("three polls must reach the source");
    assert_eq!(
        peers.len(),
        1,
        "successive energy polls should reuse the HTTP connection"
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires pinned local Mosquitto image and Docker; run explicitly"]
async fn given_oversized_energy_response_then_adapter_reports_unavailable() {
    let broker = mqtt_broker::Broker::start().await;
    let mut probe = mqtt_broker::Probe::start(broker.port).await;
    let app = Router::new().route("/api/1/energy_sites/42/live_status", get(|| async {
        Json(json!({"response":{"timestamp":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190,"padding":"x".repeat(70_000)}}))
    }));
    let (url, server) = support::serve(app).await;
    let _owned = start_adapter(&url, broker.port, "oversized-source-fixture", Some("1"));
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = probe.messages.recv().await {
            if message.topic == "homecore/devices/teslemetry_powerwall/state" {
                return false;
            }
            if message.topic == "homecore/devices/teslemetry_powerwall/availability"
                && message.payload.as_ref() == b"offline"
            {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(
        result,
        Ok(true),
        "oversized response must be rejected and reported unavailable"
    );
    server.abort();
}

#![recursion_limit = "256"]
use homecore_mitsubishi::model::{parse, Frame, Snapshot};
use serde_json::{json, Value};

fn state(value: Value) -> Snapshot {
    match parse(&serde_json::to_vec(&value).unwrap())
        .unwrap()
        .unwrap()
    {
        Frame::State(snapshot) => snapshot,
        _ => panic!("expected state"),
    }
}

#[test]
fn optional_diagnostics_and_sparse_remote_sensors_keep_units_and_identity() {
    let snapshot = state(json!({
        "type":"state","connected":true,"vane":"3","wideVane":"split",
        "compressorHz":37,"outsideTemp":12.5,"runtime":456.7,"errorCode":42,
        "subMode":"normal","stage":"gentle","autoSubMode":"leader",
        "heatThresh":19.5,"coolThresh":24.5,"uptime":321,"wifiUptime":300,"rssi":-54,
        "heapFree":120000,"heapMin":110000,"heapBlock":90000,"resetReason":"poweron",
        "crashCount":2,"wifiDrops":3,"cn105Drops":4,"epoch":1795123456,
        "pollInterval":2000,"cn105Baud":2400,"tempUnit":"F","modeMask":31,
        "roomSource":1,"roomStatus":2,"bleTimeout":600,"avgFeeding":true,
        "avgFallback":false,"effTemp":22.75,"spread":0.4,"effAge":503,
        "roomMode":1,"roomSingle":1,"roomMembers":2,"contrib":2,"exclStale":0,"exclOff":0,
        "hasLinkSensor":true,"linkTemp":21.5,"linkHumidity":52,"linkActive":true,
        "linkStale":false,"linkStaleMs":712,"remoteLive":true,"remoteBonded":true,
        "remoteRssi":-63,"remoteLastSeen":2,"remoteSyncing":false,"remoteCertState":3,
        "remoteModel":"Link","remoteFw":"0.1.5","hkReady":true,"hkPaired":true,"hkControllers":1,
        "bleEnabled":true,"bleSensors":[{"i":1,"name":"secret-name","addr":"secret-address",
            "type":"Govee","temp":22.7,"hum":48,"batt":83,"rssi":-67,"age":530,"active":true,"stale":false}],
        "remoteDials":[],"nightLat":-37.8,"nightLon":145.0,"trunc":false
    }));
    for (key, expected, unit) in [
        ("compressor_frequency", json!(37), "Hz"),
        ("outside_temperature", json!(12.5), "°C"),
        ("runtime", json!(456.7), "h"),
        ("uptime", json!(321), "s"),
        ("wifi_rssi", json!(-54), "dBm"),
        ("heap_free", json!(120000), "B"),
        ("poll_interval", json!(2000), "ms"),
        ("effective_temperature", json!(22.75), "°C"),
        ("link_temperature", json!(21.5), "°C"),
        ("ble_1_temperature", json!(22.7), "°C"),
        ("ble_1_humidity", json!(48), "%"),
        ("ble_1_battery", json!(83), "%"),
        ("ble_1_age", json!(530), "ms"),
    ] {
        assert_eq!(snapshot.values[key], expected, "{key}");
        assert_eq!(snapshot.attributes[key]["unit"], unit, "{key}");
    }
    assert_eq!(snapshot.values["ble_1_available"], true);
    assert_eq!(snapshot.values["ble_1_type"], "Govee");
    assert_eq!(snapshot.values["horizontal_vane"], "split");
    assert_eq!(snapshot.values["error_code"], 42);
    assert!(!snapshot.values.contains_key("ble_0_temperature"));
    let encoded = serde_json::to_string(&snapshot.values).unwrap();
    for secret in ["secret-name", "secret-address", "nightLat", "nightLon"] {
        assert!(!encoded.contains(secret));
    }
}

#[test]
fn controller_snapshot_exposes_current_climate_with_read_only_units() {
    let snapshot = state(json!({
        "type":"state", "connected":true, "power":true, "mode":"cool",
        "target":23.5, "room":25.2, "fan":"2", "operating":true,
        "tempUnit":"F", "hkSetupCode":"secret", "ssid":"private"
    }));
    assert_eq!(snapshot.values["cn105_connected"], true);
    assert_eq!(snapshot.values["temperature"], 25.2);
    assert_eq!(snapshot.values["setpoint"], 23.5);
    assert_eq!(snapshot.values["hvac_mode"], "cool");
    assert_eq!(snapshot.values["hvac_action"], "cooling");
    assert_eq!(snapshot.values["fan_mode"], "2");
    assert_eq!(snapshot.attributes["temperature"]["unit"], "°C");
    assert!(!snapshot.values.contains_key("hkSetupCode"));
    assert!(!snapshot.values.contains_key("ssid"));
    for (key, attribute) in &snapshot.attributes {
        assert_eq!(attribute["writable"], false, "{key}");
        assert!(snapshot.values.contains_key(key));
    }
}

#[test]
fn stale_sensors_and_missing_measurements_do_not_publish_old_values_or_zeroes() {
    let snapshot = state(json!({
        "type":"state","connected":true,"outsideTemp":null,"runtime":null,"errorCode":null,
        "linkActive":true,"linkStale":true,"linkTemp":23.4,"linkHumidity":53,
        "avgFeeding":false,"effTemp":22.1,"trunc":true,
        "bleSensors":[
            {"i":1,"active":true,"stale":true,"temp":24.1,"hum":45,"batt":71,"rssi":-67,"age":650001},
            {"i":3,"active":true,"stale":false,"temp":null,"hum":null,"batt":-1,"rssi":-72,"age":0}
        ]
    }));
    assert_eq!(snapshot.values["ble_1_available"], false);
    assert_eq!(snapshot.values["ble_1_stale"], true);
    assert_eq!(snapshot.values["ble_1_age"], 650001);
    assert_eq!(snapshot.values["snapshot_truncated"], true);
    for key in [
        "outside_temperature",
        "runtime",
        "error_code",
        "ble_1_temperature",
        "ble_1_humidity",
        "ble_1_battery",
        "ble_1_rssi",
        "ble_3_temperature",
        "ble_3_humidity",
        "ble_3_battery",
        "link_temperature",
        "link_humidity",
        "effective_temperature",
    ] {
        assert!(!snapshot.values.contains_key(key), "{key}");
        assert!(!snapshot.attributes.contains_key(key), "{key}");
    }
    let fresh = state(json!({"type":"state","connected":true,"bleSensors":[]}));
    assert!(!fresh.values.contains_key("ble_1_age"));
}

#[test]
fn device_information_has_only_bounded_read_only_public_metadata() {
    let input = json!({"type":"deviceInfo","board":"M5Stack NanoC6","fw":"v0.2.8","idf":"v5.5.4",
        "resetReason":"POWER_ON","bootCount":3,"crashTotal":1,"safeMode":false,
        "hostname":"private-host","ip":"192.0.2.12","mac":"private-mac"});
    let Some(Frame::DeviceInfo(snapshot)) = parse(&serde_json::to_vec(&input).unwrap()).unwrap()
    else {
        panic!("device info must be recognized");
    };
    assert_eq!(
        snapshot.values,
        json!({"board":"M5Stack NanoC6","firmware":"v0.2.8","idf_version":"v5.5.4",
        "reset_reason":"POWER_ON","boot_count":3,"crash_total":1,"safe_mode":false})
        .as_object()
        .unwrap()
        .clone()
    );
    assert_eq!(snapshot.attributes["boot_count"]["kind"], "integer");
    assert_eq!(snapshot.attributes["safe_mode"]["writable"], false);
}

#[test]
fn action_reports_only_supported_evidence_and_preserves_power_off() {
    for (power, mode, operating, sub_mode, auto_mode, expected_mode, action) in [
        (false, "heat", true, "normal", "off", "off", Some("off")),
        (
            true,
            "auto",
            true,
            "normal",
            "heat",
            "auto",
            Some("heating"),
        ),
        (true, "auto", true, "normal", "leader", "auto", None),
        (true, "auto", true, "normal", "off", "auto", None),
        (true, "heat", true, "defrost", "off", "heat", None),
        (true, "heat", true, "preheat", "off", "heat", None),
        (true, "dry", false, "normal", "off", "dry", Some("idle")),
        (true, "fan", false, "normal", "off", "fan_only", Some("fan")),
    ] {
        let snapshot = state(
            json!({"type":"state","connected":true,"power":power,"mode":mode,
            "operating":operating,"subMode":sub_mode,"autoSubMode":auto_mode}),
        );
        assert_eq!(snapshot.values["hvac_mode"], expected_mode);
        assert_eq!(
            snapshot.values.get("hvac_action").and_then(Value::as_str),
            action,
            "{mode} {sub_mode} {auto_mode}"
        );
    }
}

#[test]
fn room_calibration_and_dial_readings_preserve_units_without_addresses() {
    let snapshot = state(json!({"type":"state","connected":true,
        "roomOffs":[0, -9, 12, 0, 0, 0],
        "remoteDials":[{"mac":"private-address","live":true,"seen":3,"t":21.75},
            {"mac":"private-address-2","live":false,"seen":-1,"t":null}]
    }));
    assert_eq!(snapshot.values["room_offset_1"], -0.9);
    assert_eq!(snapshot.attributes["room_offset_1"]["unit"], "°C");
    assert_eq!(snapshot.values["remote_dial_count"], 2);
    assert_eq!(snapshot.values["remote_dial_0_temperature"], 21.75);
    assert_eq!(snapshot.values["remote_dial_0_last_seen"], 3);
    assert_eq!(snapshot.values["remote_dial_1_live"], false);
    assert!(!snapshot.values.contains_key("remote_dial_1_temperature"));
    assert!(!snapshot.values.contains_key("remote_dial_1_last_seen"));
    assert!(!serde_json::to_string(&snapshot.values)
        .unwrap()
        .contains("private-address"));
}

#[test]
fn unknown_clock_and_remote_age_sentinels_are_not_measurements() {
    let snapshot = state(json!({"type":"state","connected":false,"epoch":0,"remoteLastSeen":-1}));
    assert_eq!(snapshot.values["cn105_connected"], false);
    assert!(!snapshot.values.contains_key("device_epoch"));
    assert!(!snapshot.values.contains_key("remote_last_seen"));
}

#[test]
fn fault_distinguishes_reported_clear_fault_from_absent_diagnostics() {
    let clear = state(json!({"type":"state","connected":true,"errorCode":null}));
    assert_eq!(clear.values["fault"], false);
    assert!(!clear.values.contains_key("error_code"));
    let failed = state(json!({"type":"state","connected":true,"errorCode":42}));
    assert_eq!(failed.values["fault"], true);
    assert_eq!(failed.values["error_code"], 42);
    assert_eq!(failed.attributes["fault"]["kind"], "bool");
    let missing = state(json!({"type":"state","connected":true,"trunc":true}));
    assert!(!missing.values.contains_key("fault"));
}

#[test]
fn malformed_known_fields_are_rejected_without_echoing_input() {
    let bad_frames = [
        json!([]),
        json!({}),
        json!({"type":"state"}),
        json!({"type":"state","connected":"secret"}),
        json!({"type":"state","connected":true,"room":"secret"}),
        json!({"type":"state","connected":true,"compressorHz":-1}),
        json!({"type":"state","connected":true,"mode":"secret"}),
        json!({"type":"state","connected":true,"runtime":-0.1}),
        json!({"type":"state","connected":true,"roomOffs":[0,0]}),
        json!({"type":"state","connected":true,"roomOffs":[0,0,0,0,0,100]}),
        json!({"type":"state","connected":true,"bleSensors":[{"i":4}]}),
        json!({"type":"state","connected":true,"bleSensors":[{"i":1},{"i":1}]}),
        json!({"type":"state","connected":true,"bleSensors":[{"i":0,"hum":101}]}),
        json!({"type":"state","connected":true,"bleSensors":[{"i":0,"batt":-2}]}),
        json!({"type":"state","connected":true,"remoteDials":[null]}),
        json!({"type":"deviceInfo","board":"x".repeat(129)}),
        json!({"type":"deviceInfo","fw":"secret\nvalue"}),
        json!({"type":"deviceInfo","bootCount":1.5}),
    ];
    for frame in bad_frames {
        let error = parse(&serde_json::to_vec(&frame).unwrap()).expect_err("bad frame accepted");
        assert!(!error.contains("secret"));
    }
    assert!(parse(&vec![b' '; 32769]).is_err());
    assert!(parse(b"{bad secret}").is_err());
    assert!(parse(b"{\"type\":\"log\",\"msg\":\"secret\"}")
        .unwrap()
        .is_none());
    assert!(parse(b"{\"type\":\"future-extension\"}").unwrap().is_none());
}

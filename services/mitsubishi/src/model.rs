//! Read-only projection of mitsubishi-cn105-homekit v0.2.8 WebSocket snapshots.
use serde_json::{json, Map, Value};

pub const MAX_FRAME_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub enum Frame {
    State(Snapshot),
    DeviceInfo(Snapshot),
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub values: Map<String, Value>,
    pub attributes: Map<String, Value>,
}

enum AttributeKind {
    Bool,
    Float,
    Integer,
    String,
}

impl AttributeKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Float => "float",
            Self::Integer => "integer",
            Self::String => "string",
        }
    }
}

impl Snapshot {
    fn new() -> Self {
        Self {
            values: Map::new(),
            attributes: Map::new(),
        }
    }

    fn insert(&mut self, key: &str, value: Value, kind: AttributeKind, unit: Option<&str>) {
        let mut attribute =
            json!({"kind":kind.as_str(),"writable":false,"display_name":key.replace('_', " ")});
        if let Some(unit) = unit {
            attribute["unit"] = unit.into();
        }
        self.values.insert(key.into(), value);
        self.attributes.insert(key.into(), attribute);
    }

    fn remove(&mut self, key: &str) {
        self.values.remove(key);
        self.attributes.remove(key);
    }

    fn boolean(&mut self, input: &Value, source: &str, key: &str) -> Result<(), &'static str> {
        if let Some(value) = optional(input, source) {
            let value = value.as_bool().ok_or("invalid boolean")?;
            self.insert(key, value.into(), AttributeKind::Bool, None);
        }
        Ok(())
    }

    fn number(
        &mut self,
        input: &Value,
        source: &str,
        key: &str,
        unit: Option<&str>,
        min: f64,
        max: f64,
    ) -> Result<(), &'static str> {
        if let Some(value) = optional(input, source) {
            let number = value.as_f64().ok_or("invalid number")?;
            if !number.is_finite() || number < min || number > max {
                return Err("number outside bounds");
            }
            self.insert(key, value.clone(), AttributeKind::Float, unit);
        }
        Ok(())
    }

    fn choice(
        &mut self,
        input: &Value,
        source: &str,
        key: &str,
        choices: &[&str],
    ) -> Result<(), &'static str> {
        if let Some(value) = optional(input, source) {
            let value = value.as_str().ok_or("invalid enum")?;
            if !choices.contains(&value) {
                return Err("invalid enum");
            }
            self.insert(key, value.into(), AttributeKind::String, None);
        }
        Ok(())
    }

    fn integer(
        &mut self,
        input: &Value,
        source: &str,
        key: &str,
        unit: Option<&str>,
        min: i64,
        max: i64,
    ) -> Result<(), &'static str> {
        if let Some(value) = optional(input, source) {
            let number = value.as_i64().ok_or("invalid integer")?;
            if !(min..=max).contains(&number) {
                return Err("integer outside bounds");
            }
            self.insert(key, number.into(), AttributeKind::Integer, unit);
        }
        Ok(())
    }

    fn text(&mut self, input: &Value, source: &str, key: &str) -> Result<(), &'static str> {
        if let Some(value) = optional(input, source) {
            let value = value.as_str().ok_or("invalid string")?;
            if value.len() > 128 || value.chars().any(char::is_control) {
                return Err("invalid string");
            }
            self.insert(key, value.into(), AttributeKind::String, None);
        }
        Ok(())
    }
}

fn optional<'a>(input: &'a Value, key: &str) -> Option<&'a Value> {
    input.get(key).filter(|value| !value.is_null())
}

fn state(input: &Value) -> Result<Snapshot, &'static str> {
    let connected = input
        .get("connected")
        .and_then(Value::as_bool)
        .ok_or("missing connection state")?;
    let mut output = Snapshot::new();
    output.insert(
        "cn105_connected",
        connected.into(),
        AttributeKind::Bool,
        None,
    );
    output.boolean(input, "power", "power")?;
    output.boolean(input, "operating", "operating")?;
    output.number(input, "room", "temperature", Some("°C"), -100.0, 150.0)?;
    output.number(input, "target", "setpoint", Some("°C"), -100.0, 150.0)?;
    output.choice(
        input,
        "mode",
        "operation_mode",
        &["heat", "cool", "dry", "fan", "auto", "unknown"],
    )?;
    output.choice(
        input,
        "fan",
        "fan_mode",
        &["auto", "quiet", "1", "2", "3", "4", "unknown"],
    )?;
    optional_readings(&mut output, input)?;
    // The firmware emits null exactly when its hasError flag is false.
    if let Some(error) = input.get("errorCode") {
        output.insert(
            "fault",
            (!error.is_null()).into(),
            AttributeKind::Bool,
            None,
        );
    }
    ble_sensors(&mut output, input)?;
    room_arrays(&mut output, input)?;
    if let Some(count) = output
        .values
        .get("vane_configuration")
        .and_then(Value::as_u64)
    {
        output.insert(
            "has_vertical_vane",
            (count >= 1).into(),
            AttributeKind::Bool,
            None,
        );
        output.insert(
            "has_horizontal_vane",
            (count >= 2).into(),
            AttributeKind::Bool,
            None,
        );
    }
    hvac_status(&mut output, input);
    Ok(output)
}

fn hvac_status(output: &mut Snapshot, input: &Value) {
    let power = input.get("power").and_then(Value::as_bool);
    let mode = input.get("mode").and_then(Value::as_str);
    let mode = match (power, mode) {
        (Some(false), _) => Some("off"),
        (Some(true), Some("fan")) => Some("fan_only"),
        (Some(true), Some(mode @ ("heat" | "cool" | "dry" | "auto"))) => Some(mode),
        _ => None,
    };
    if let Some(mode) = mode {
        output.insert("hvac_mode", mode.into(), AttributeKind::String, None);
        let actual_mode = if mode == "auto" {
            input
                .get("autoSubMode")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        } else {
            mode
        };
        let transitional = matches!(
            input.get("subMode").and_then(Value::as_str),
            Some("defrost" | "preheat" | "standby" | "unknown")
        );
        let action = match (actual_mode, input.get("operating").and_then(Value::as_bool)) {
            ("off", _) if mode == "off" => Some("off"),
            ("fan_only", _) => Some("fan"),
            (_, Some(false)) => Some("idle"),
            (_, Some(true)) if transitional => None,
            ("cool", Some(true)) => Some("cooling"),
            ("heat", Some(true)) => Some("heating"),
            ("dry", Some(true)) => Some("drying"),
            _ => None,
        };
        if let Some(action) = action {
            output.insert("hvac_action", action.into(), AttributeKind::String, None);
        }
    }
}

fn optional_readings(output: &mut Snapshot, input: &Value) -> Result<(), &'static str> {
    for (source, key, choices) in [
        (
            "vane",
            "vertical_vane",
            &["auto", "1", "2", "3", "4", "5", "swing", "unknown"][..],
        ),
        (
            "wideVane",
            "horizontal_vane",
            &["ll", "l", "c", "r", "rr", "split", "swing", "unknown"][..],
        ),
        (
            "subMode",
            "sub_mode",
            &["normal", "defrost", "preheat", "standby", "unknown"][..],
        ),
        (
            "stage",
            "stage",
            &[
                "idle", "low", "gentle", "medium", "moderate", "high", "diffuse", "unknown",
            ][..],
        ),
        (
            "autoSubMode",
            "auto_sub_mode",
            &["off", "cool", "heat", "leader", "unknown"][..],
        ),
        ("tempUnit", "display_temperature_unit", &["C", "F"][..]),
    ] {
        output.choice(input, source, key, choices)?;
    }
    for (source, key) in [
        ("outsideTemp", "outside_temperature"),
        ("heatThresh", "heating_setpoint"),
        ("coolThresh", "cooling_setpoint"),
        ("effTemp", "effective_temperature"),
        ("linkTemp", "link_temperature"),
    ] {
        output.number(input, source, key, Some("°C"), -100.0, 150.0)?;
    }
    output.number(input, "runtime", "runtime", Some("h"), 0.0, 1e9)?;
    output.number(
        input,
        "spread",
        "temperature_spread",
        Some("°C"),
        0.0,
        250.0,
    )?;
    output.number(
        input,
        "linkHumidity",
        "link_humidity",
        Some("%"),
        0.0,
        100.0,
    )?;
    read_integer_diagnostics(output, input)?;
    for (source, key) in [
        ("avgFeeding", "room_feed_active"),
        ("avgFallback", "room_feed_fallback"),
        ("hasLinkSensor", "link_sensor_present"),
        ("linkActive", "link_active"),
        ("linkStale", "link_stale"),
        ("remoteBonded", "remote_bonded"),
        ("remoteLive", "remote_live"),
        ("remotePairing", "remote_pairing"),
        ("remoteSyncing", "remote_syncing"),
        ("hkReady", "homekit_ready"),
        ("hkPaired", "homekit_paired"),
        ("bleEnabled", "ble_enabled"),
        ("bleDiscovering", "ble_discovering"),
        ("bleReverted", "ble_reverted"),
        ("betaChannel", "beta_channel"),
        ("trunc", "snapshot_truncated"),
    ] {
        output.boolean(input, source, key)?;
    }
    for (source, key) in [
        ("resetReason", "reset_reason"),
        ("hkStatus", "homekit_status"),
        ("remoteModel", "remote_model"),
        ("remoteFw", "remote_firmware"),
        ("remotePairResult", "remote_pairing_result"),
    ] {
        output.text(input, source, key)?;
    }
    if input["linkActive"] != true || input["linkStale"] != false {
        output.remove("link_temperature");
        output.remove("link_humidity");
    }
    if input["avgFeeding"] != true {
        output.remove("effective_temperature");
    }
    if input["epoch"] == 0 {
        output.remove("device_epoch");
    }
    if input["remoteLastSeen"] == -1 {
        output.remove("remote_last_seen");
    }
    Ok(())
}

fn read_integer_diagnostics(output: &mut Snapshot, input: &Value) -> Result<(), &'static str> {
    for (source, key, unit, min, max) in [
        ("compressorHz", "compressor_frequency", Some("Hz"), 0, 255),
        ("errorCode", "error_code", None, 0, 255),
        ("rssi", "wifi_rssi", Some("dBm"), -127, 0),
        ("remoteRssi", "remote_rssi", Some("dBm"), -127, 0),
        ("pollInterval", "poll_interval", Some("ms"), 1, 30000),
        ("cn105Baud", "cn105_baud", Some("Bd"), 2400, 9600),
        ("bleTimeout", "sensor_timeout", Some("s"), 30, 3600),
        ("vaneConfig", "vane_configuration", None, 0, 255),
        ("modeMask", "mode_capabilities", None, 0, 31),
        ("modeMaskBoot", "boot_mode_capabilities", None, 0, 31),
        ("roomSource", "room_source", None, 0, 255),
        ("roomStatus", "room_status", None, 0, 255),
        ("roomMode", "room_mode", None, 0, 1),
        ("roomSingle", "room_single", None, 0, 5),
        ("roomMembers", "room_members", None, 0, 63),
        ("contrib", "room_contributors", None, 0, 63),
        ("exclStale", "room_excluded_stale", None, 0, 63),
        ("exclOff", "room_excluded_disabled", None, 0, 63),
        (
            "remoteLastSeen",
            "remote_last_seen",
            Some("s"),
            -1,
            i64::from(u32::MAX),
        ),
        ("remoteCertState", "remote_certificate_state", None, 0, 3),
        (
            "remotePairSecs",
            "remote_pairing_remaining",
            Some("s"),
            0,
            60,
        ),
        ("hkControllers", "homekit_controllers", None, 0, 255),
        ("logLevel", "log_level", None, 0, 7),
        (
            "nightDuskOff",
            "night_dusk_offset",
            Some("min"),
            -1440,
            1440,
        ),
        (
            "nightDawnOff",
            "night_dawn_offset",
            Some("min"),
            -1440,
            1440,
        ),
        ("nightCeil", "night_ceiling", Some("%"), 0, 100),
    ] {
        output.integer(input, source, key, unit, min, max)?;
    }
    for (source, key, unit) in [
        ("uptime", "uptime", "s"),
        ("wifiUptime", "wifi_uptime", "s"),
        ("heapFree", "heap_free", "B"),
        ("heapMin", "heap_minimum", "B"),
        ("heapBlock", "heap_largest_block", "B"),
        ("epoch", "device_epoch", "s"),
        ("effAge", "effective_temperature_age", "ms"),
        ("linkStaleMs", "link_age", "ms"),
    ] {
        output.integer(input, source, key, Some(unit), 0, i64::from(u32::MAX))?;
    }
    for (source, key) in [
        ("crashCount", "crash_count"),
        ("wifiDrops", "wifi_disconnects"),
        ("cn105Drops", "cn105_disconnects"),
    ] {
        output.integer(input, source, key, None, 0, i64::from(u32::MAX))?;
    }
    Ok(())
}

fn ble_sensors(output: &mut Snapshot, input: &Value) -> Result<(), &'static str> {
    let Some(sensors) = optional(input, "bleSensors") else {
        return Ok(());
    };
    let sensors = sensors.as_array().ok_or("invalid sensor list")?;
    if sensors.len() > 4 {
        return Err("too many sensors");
    }
    let mut used = [false; 4];
    for sensor in sensors {
        let index = sensor
            .get("i")
            .and_then(Value::as_u64)
            .filter(|i| *i < 4)
            .ok_or("invalid sensor slot")? as usize;
        if used[index] {
            return Err("duplicate sensor slot");
        }
        used[index] = true;
        let key = |suffix: &str| format!("ble_{index}_{suffix}");
        output.boolean(sensor, "active", &key("active"))?;
        output.boolean(sensor, "stale", &key("stale"))?;
        output.insert(
            &key("available"),
            (sensor["active"] == true && sensor["stale"] == false).into(),
            AttributeKind::Bool,
            None,
        );
        output.integer(
            sensor,
            "age",
            &key("age"),
            Some("ms"),
            0,
            i64::from(u32::MAX),
        )?;
        output.text(sensor, "type", &key("type"))?;
        output.number(
            sensor,
            "temp",
            &key("temperature"),
            Some("°C"),
            -100.0,
            150.0,
        )?;
        output.number(sensor, "hum", &key("humidity"), Some("%"), 0.0, 100.0)?;
        output.integer(sensor, "rssi", &key("rssi"), Some("dBm"), -127, 0)?;
        if sensor.get("batt") != Some(&json!(-1)) {
            output.integer(sensor, "batt", &key("battery"), Some("%"), 0, 100)?;
        }
        if sensor["active"] != true || sensor["stale"] != false || input["bleEnabled"] == false {
            output.insert(&key("available"), false.into(), AttributeKind::Bool, None);
            for suffix in ["temperature", "humidity", "battery", "rssi"] {
                output.remove(&key(suffix));
            }
        }
    }
    Ok(())
}

fn room_arrays(output: &mut Snapshot, input: &Value) -> Result<(), &'static str> {
    if let Some(offsets) = optional(input, "roomOffs") {
        let offsets = offsets
            .as_array()
            .filter(|v| v.len() == 6)
            .ok_or("invalid room offsets")?;
        for (index, value) in offsets.iter().enumerate() {
            let value = value
                .as_i64()
                .filter(|v| (-99..=99).contains(v))
                .ok_or("invalid room offset")?;
            output.insert(
                &format!("room_offset_{index}"),
                (value as f64 / 10.0).into(),
                AttributeKind::Float,
                Some("°C"),
            );
        }
    }
    if let Some(dials) = optional(input, "remoteDials") {
        let dials = dials
            .as_array()
            .filter(|v| v.len() <= 4)
            .ok_or("invalid dial list")?;
        output.insert(
            "remote_dial_count",
            dials.len().into(),
            AttributeKind::Integer,
            None,
        );
        for (index, dial) in dials.iter().enumerate() {
            dial.as_object().ok_or("invalid dial")?;
            let key = |suffix: &str| format!("remote_dial_{index}_{suffix}");
            output.boolean(dial, "live", &key("live"))?;
            output.integer(
                dial,
                "seen",
                &key("last_seen"),
                Some("s"),
                -1,
                i64::from(u32::MAX),
            )?;
            if dial["seen"] == -1 {
                output.remove(&key("last_seen"));
            }
            output.number(dial, "t", &key("temperature"), Some("°C"), -100.0, 150.0)?;
            if dial["live"] != true {
                output.remove(&key("temperature"));
            }
        }
    }
    Ok(())
}

fn device_info(input: &Value) -> Result<Snapshot, &'static str> {
    let mut output = Snapshot::new();
    for (source, key) in [
        ("board", "board"),
        ("fw", "firmware"),
        ("idf", "idf_version"),
        ("resetReason", "reset_reason"),
    ] {
        output.text(input, source, key)?;
    }
    for (source, key) in [("bootCount", "boot_count"), ("crashTotal", "crash_total")] {
        output.integer(input, source, key, None, 0, i64::from(u32::MAX))?;
    }
    output.boolean(input, "safeMode", "safe_mode")?;
    Ok(output)
}

pub fn parse(bytes: &[u8]) -> Result<Option<Frame>, &'static str> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err("frame too large");
    }
    let input: Value = serde_json::from_slice(bytes).map_err(|_| "invalid JSON")?;
    input.as_object().ok_or("expected object")?;
    match input
        .get("type")
        .and_then(Value::as_str)
        .ok_or("missing frame type")?
    {
        "state" => state(&input).map(|value| Some(Frame::State(value))),
        "deviceInfo" => device_info(&input).map(|value| Some(Frame::DeviceInfo(value))),
        _ => Ok(None),
    }
}

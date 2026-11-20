//! Projection of Electrolux Climate status into validated climate readings.
use serde_json::Value;

#[derive(Debug, PartialEq)]
pub struct ClimateStatus {
    pub current_temperature: f64,
    pub target_temperature: f64,
    pub mode: &'static str,
    pub fan: &'static str,
    pub swing: bool,
}

pub fn parse_status(value: &Value) -> Result<ClimateStatus, &'static str> {
    let number = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .ok_or("Invalid climate reading")
    };
    let integer = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_u64)
            .ok_or("Invalid climate reading")
    };
    let current_temperature = number("envtemp")?;
    let target_temperature = number("temp")?;
    if !(0.0..=40.0).contains(&target_temperature) {
        return Err("Invalid climate reading");
    }
    let mode = match integer("ac_mode")? {
        0 => "cool",
        1 => "heat",
        2 => "dry",
        3 => "fan_only",
        4 => "auto",
        6 => "heat_cool",
        _ => return Err("Invalid climate reading"),
    };
    let mode = match integer("ac_pwr")? {
        0 => "off",
        1 => mode,
        _ => return Err("Invalid climate reading"),
    };
    let fan = match integer("ac_mark")? {
        0 => "auto",
        1 => "low",
        2 => "medium",
        3 => "high",
        4 => "turbo",
        5 => "quiet",
        _ => return Err("Invalid climate reading"),
    };
    let swing = match integer("ac_vdir")? {
        0 => false,
        1 => true,
        _ => return Err("Invalid climate reading"),
    };
    Ok(ClimateStatus {
        current_temperature,
        target_temperature,
        mode,
        fan,
        swing,
    })
}

pub mod protocol;

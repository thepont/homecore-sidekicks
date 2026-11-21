//! Writable fields and command validation use the same controller capabilities.
use crate::model::Snapshot;
use serde::{de, Deserialize, Deserializer};
use serde_json::{json, Map, Value};
use std::fmt;
use tokio::time::Instant;

pub(crate) const MAX_COMMAND_BYTES: usize = 1024;
const MIN_TEMP: f64 = 16.0;
const MAX_SINGLE: f64 = 30.0;
const MAX_AUTO: f64 = 30.5;
const AUTO_GAP: f64 = 2.0;

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Off,
    Heat,
    Cool,
    Auto,
    Dry,
    FanOnly,
}

impl Mode {
    // The firmware's mode_caps.h defines Auto as dependent on both Heat and Cool.
    const OPTIONS: [(Self, &str, &str, u64); 6] = [
        (Self::Off, "off", "off", 0),
        (Self::Heat, "heat", "heat", 1 << 0),
        (Self::Cool, "cool", "cool", 1 << 1),
        (Self::Auto, "auto", "auto", 0b1_0011),
        (Self::Dry, "dry", "dry", 1 << 2),
        (Self::FanOnly, "fan_only", "fan", 1 << 3),
    ];
}

pub(crate) enum Command {
    HvacMode(Mode),
    Setpoint(f64),
    HeatSetpoint(f64),
    CoolSetpoint(f64),
    FanMode(String),
    VerticalVane(String),
    HorizontalVane(String),
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum CommandField {
    HvacMode,
    Setpoint,
    HeatSetpoint,
    CoolSetpoint,
    FanMode,
    VerticalVane,
    HorizontalVane,
    #[serde(rename = "_hc")]
    Context,
    CorrelationId,
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CommandVisitor;
        impl<'de> de::Visitor<'de> for CommandVisitor {
            type Value = Command;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("one HomeCore climate setting")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Command, M::Error> {
                let mut command = None;
                let mut saw_context = false;
                let mut saw_correlation = false;
                while let Some(key) = map.next_key::<CommandField>()? {
                    // Core adds these fields when publishing a command. Their
                    // contents are neither interpreted nor forwarded to the ESP32.
                    let next = match key {
                        CommandField::Context if !saw_context => {
                            saw_context = true;
                            map.next_value::<de::IgnoredAny>()?;
                            continue;
                        }
                        CommandField::CorrelationId if !saw_correlation => {
                            saw_correlation = true;
                            map.next_value::<de::IgnoredAny>()?;
                            continue;
                        }
                        CommandField::Context => return Err(de::Error::duplicate_field("_hc")),
                        CommandField::CorrelationId => {
                            return Err(de::Error::duplicate_field("correlation_id"))
                        }
                        CommandField::HvacMode => Command::HvacMode(map.next_value()?),
                        CommandField::Setpoint => Command::Setpoint(map.next_value()?),
                        CommandField::HeatSetpoint => Command::HeatSetpoint(map.next_value()?),
                        CommandField::CoolSetpoint => Command::CoolSetpoint(map.next_value()?),
                        CommandField::FanMode => Command::FanMode(map.next_value()?),
                        CommandField::VerticalVane => Command::VerticalVane(map.next_value()?),
                        CommandField::HorizontalVane => Command::HorizontalVane(map.next_value()?),
                    };
                    if command.replace(next).is_some() {
                        return Err(de::Error::custom("expected one climate setting"));
                    }
                }
                command.ok_or_else(|| de::Error::custom("missing climate setting"))
            }
        }

        deserializer.deserialize_map(CommandVisitor)
    }
}

pub(crate) struct ReceivedCommand {
    pub(crate) command: Command,
    pub(crate) received: Instant,
}

pub(crate) struct ControllerWrite {
    pub(crate) frame: Value,
    field: &'static str,
    expected: Value,
}

impl ControllerWrite {
    fn new(frame: Value, field: &'static str, expected: Value) -> Self {
        Self {
            frame,
            field,
            expected,
        }
    }

    pub(crate) fn reported_by(&self, snapshot: &Snapshot) -> bool {
        let Some(reported) = snapshot.values.get(self.field) else {
            return false;
        };
        if let Some(wanted) = self.expected.as_f64() {
            reported.as_f64() == Some(wanted)
        } else {
            *reported == self.expected
        }
    }
}

struct Airflow {
    field: &'static str,
    wire: &'static str,
    capability: Option<&'static str>,
    options: &'static [&'static str],
}

impl Airflow {
    const FAN: Self = Self {
        field: "fan_mode",
        wire: "fan",
        capability: None,
        options: &["auto", "quiet", "1", "2", "3", "4"],
    };
    const VERTICAL: Self = Self {
        field: "vertical_vane",
        wire: "vane",
        capability: Some("has_vertical_vane"),
        options: &["auto", "1", "2", "3", "4", "5", "swing"],
    };
    const HORIZONTAL: Self = Self {
        field: "horizontal_vane",
        wire: "wideVane",
        capability: Some("has_horizontal_vane"),
        options: &["ll", "l", "c", "r", "rr", "split", "swing"],
    };
}

pub(crate) struct Controls<'a> {
    values: &'a Map<String, Value>,
    mask: u64,
    mode: Mode,
}

impl<'a> Controls<'a> {
    pub(crate) fn from_snapshot(snapshot: &'a Snapshot) -> Option<Self> {
        let values = &snapshot.values;
        if !values.get("cn105_connected")?.as_bool()? {
            return None;
        }
        let mask = values.get("mode_capabilities")?.as_u64()?;
        let current = values.get("hvac_mode")?.as_str()?;
        let mode = Mode::OPTIONS
            .iter()
            .find(|(_, key, _, _)| *key == current)?
            .0;
        Some(Self { values, mask, mode })
    }

    fn supported_modes(&self) -> impl Iterator<Item = (Mode, &'static str, &'static str)> + '_ {
        Mode::OPTIONS
            .into_iter()
            .filter(|(_, _, _, required)| (self.mask & required) == *required)
            .map(|(mode, key, wire, _)| (mode, key, wire))
    }

    fn supports(&self, mode: Mode) -> bool {
        self.supported_modes().any(|(option, _, _)| option == mode)
    }

    fn supports_airflow(&self, airflow: &Airflow) -> bool {
        self.values.contains_key(airflow.field)
            && airflow
                .capability
                .is_none_or(|key| self.values.get(key).and_then(Value::as_bool) == Some(true))
    }

    fn auto_range(&self) -> Option<(f64, f64)> {
        if !self.supports(Mode::Auto) {
            return None;
        }
        let heat = self.values.get("heating_setpoint")?.as_f64()?;
        let cool = self.values.get("cooling_setpoint")?.as_f64()?;
        (valid_temp(heat, MAX_AUTO, 0.5)
            && valid_temp(cool, MAX_AUTO, 0.5)
            && cool - heat >= AUTO_GAP)
            .then_some((heat, cool))
    }

    pub(crate) fn describe(
        &self,
        values: &mut Map<String, Value>,
        attributes: &mut Map<String, Value>,
    ) {
        let modes: Vec<_> = self.supported_modes().map(|(_, key, _)| key).collect();
        if let Some(attribute) = attributes.get_mut("hvac_mode") {
            set_choices(attribute, &modes);
        }
        if self.supports(Mode::Heat) || self.supports(Mode::Cool) {
            if let Some(attribute) = attributes.get_mut("setpoint") {
                set_temperature(attribute, MAX_SINGLE, 1.0);
            }
        }
        for airflow in [Airflow::FAN, Airflow::VERTICAL, Airflow::HORIZONTAL] {
            if self.supports_airflow(&airflow) {
                if let Some(attribute) = attributes.get_mut(airflow.field) {
                    set_choices(attribute, airflow.options);
                }
            }
        }
        if let Some((heat, cool)) = self.auto_range() {
            for (key, label, value) in [
                ("heat_setpoint", "Heat to", heat),
                ("cool_setpoint", "Cool to", cool),
            ] {
                let mut attribute = json!({"kind":"float","unit":"°C","display_name":label});
                set_temperature(&mut attribute, MAX_AUTO, 0.5);
                values.insert(key.into(), json!(value));
                attributes.insert(key.into(), attribute);
            }
            values.insert("min_setpoint_gap".into(), json!(AUTO_GAP as u8));
            attributes.insert(
                "min_setpoint_gap".into(),
                json!({"kind":"float","unit":"°C","display_name":"Minimum Auto gap","writable":false}),
            );
        }
    }

    pub(crate) fn frame(&self, command: Command) -> Option<ControllerWrite> {
        match command {
            Command::HvacMode(mode) if mode != self.mode => {
                let (_, key, wire) = self
                    .supported_modes()
                    .find(|(option, _, _)| *option == mode)?;
                let frame = if mode == Mode::Off {
                    json!({"cmd":"set","power":false})
                } else {
                    json!({"cmd":"set","mode":wire,"power":true})
                };
                Some(ControllerWrite::new(frame, "hvac_mode", key.into()))
            }
            Command::Setpoint(value)
                if matches!(self.mode, Mode::Heat | Mode::Cool)
                    && self.supports(self.mode)
                    && self.values.contains_key("setpoint")
                    && valid_temp(value, MAX_SINGLE, 1.0) =>
            {
                // Whole degrees work with both CN105 temperature encodings.
                Some(ControllerWrite::new(
                    json!({"cmd":"set","target":value as u8}),
                    "setpoint",
                    value.into(),
                ))
            }
            Command::HeatSetpoint(value) if self.mode == Mode::Auto => {
                let (_, cool) = self.auto_range()?;
                (valid_temp(value, MAX_AUTO, 0.5) && cool - value >= AUTO_GAP).then(|| {
                    ControllerWrite::new(
                        json!({"cmd":"set","heatThresh":value}),
                        "heating_setpoint",
                        value.into(),
                    )
                })
            }
            Command::CoolSetpoint(value) if self.mode == Mode::Auto => {
                let (heat, _) = self.auto_range()?;
                (valid_temp(value, MAX_AUTO, 0.5) && value - heat >= AUTO_GAP).then(|| {
                    ControllerWrite::new(
                        json!({"cmd":"set","coolThresh":value}),
                        "cooling_setpoint",
                        value.into(),
                    )
                })
            }
            Command::FanMode(value) => self.airflow_frame(Airflow::FAN, &value),
            Command::VerticalVane(value) => self.airflow_frame(Airflow::VERTICAL, &value),
            Command::HorizontalVane(value) => self.airflow_frame(Airflow::HORIZONTAL, &value),
            _ => None,
        }
    }

    fn airflow_frame(&self, airflow: Airflow, value: &str) -> Option<ControllerWrite> {
        (self.mode != Mode::Off
            && self.supports_airflow(&airflow)
            && airflow.options.contains(&value))
        .then(|| {
            ControllerWrite::new(
                json!({"cmd":"set",(airflow.wire):value}),
                airflow.field,
                value.into(),
            )
        })
    }
}

fn valid_temp(value: f64, max: f64, step: f64) -> bool {
    (MIN_TEMP..=max).contains(&value) && ((value - MIN_TEMP) / step).fract() == 0.0
}

fn set_temperature(attribute: &mut Value, max: f64, step: f64) {
    attribute["writable"] = true.into();
    attribute["min"] = MIN_TEMP.into();
    attribute["max"] = max.into();
    attribute["step"] = step.into();
}

fn set_choices(attribute: &mut Value, choices: &[&str]) {
    attribute["kind"] = "enum".into();
    attribute["writable"] = true.into();
    attribute["options"] = json!(choices);
}

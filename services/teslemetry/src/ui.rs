//! Teslemetry's slot declarations for the shared homeCore asset server.

pub use homecore_plugin_ui::{respond_unavailable, UiError, MAX_CHUNK_BYTES, MAX_REQUEST_BYTES};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug)]
pub struct UiAssets(homecore_plugin_ui::UiAssets);

impl UiAssets {
    pub fn load(dir: &Path, version: &str) -> Result<Self, UiError> {
        homecore_plugin_ui::UiAssets::load(dir, version, validate_slots).map(Self)
    }

    pub fn descriptor(&self) -> Value {
        self.0.descriptor()
    }

    pub fn respond(&self, payload: &[u8]) -> Option<Value> {
        self.0.respond(payload)
    }
}

fn validate_slots(slots: &[Value]) -> Result<(), UiError> {
    if slots.is_empty() || slots.len() > 5 {
        return Err(UiError);
    }
    for (index, value) in slots.iter().enumerate() {
        let slot = value.as_object().ok_or(UiError)?;
        let element = slot.get("element").and_then(Value::as_str).ok_or(UiError)?;
        if slots[..index]
            .iter()
            .any(|previous| previous.get("element") == Some(&json!(element)))
        {
            return Err(UiError);
        }
        match (slot.get("slot").and_then(Value::as_str), element) {
            (Some("device.card"), "hc-teslemetry-card") => {
                if slot
                    .keys()
                    .any(|k| !["slot", "element", "device_types", "forms"].contains(&k.as_str()))
                    || slot
                        .get("device_types")
                        .is_some_and(|v| v != &json!(["sensor"]))
                {
                    return Err(UiError);
                }
            }
            (
                Some("dashboard.card"),
                "hc-teslemetry-solar"
                | "hc-teslemetry-grid"
                | "hc-teslemetry-battery"
                | "hc-teslemetry-combined",
            ) => {
                let label = match element {
                    "hc-teslemetry-solar" => "Solar",
                    "hc-teslemetry-grid" => "Grid",
                    "hc-teslemetry-combined" => "Combined energy",
                    _ => "Battery",
                };
                if slot
                    .keys()
                    .any(|k| !["slot", "element", "label", "forms"].contains(&k.as_str()))
                    || slot.get("label") != Some(&json!(label))
                {
                    return Err(UiError);
                }
            }
            _ => return Err(UiError),
        }
        if let Some(forms) = slot.get("forms") {
            let forms = forms.as_array().ok_or(UiError)?;
            if forms.is_empty()
                || forms.len() > 3
                || forms.iter().enumerate().any(|(i, f)| {
                    !matches!(f.as_str(), Some("compact" | "standard" | "large"))
                        || forms[..i].contains(f)
                })
            {
                return Err(UiError);
            }
        }
    }
    Ok(())
}

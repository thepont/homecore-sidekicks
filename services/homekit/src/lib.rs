use serde_json::{json, Map, Value};

fn hap_type(value: &Value, short: &str, full: &str) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(short) || s.eq_ignore_ascii_case(full))
}

/// Translate a HomeKit contact accessory into homeCore's sensor vocabulary.
pub fn contact_state(accessory: &Value) -> Option<Value> {
    let services = accessory.get("services")?.as_array()?;
    let mut contact = None;
    let mut battery = None;
    for service in services {
        let kind = service.get("type")?;
        let is_contact = hap_type(kind, "80", "00000080-0000-1000-8000-0026BB765291");
        let is_battery = hap_type(kind, "96", "00000096-0000-1000-8000-0026BB765291");
        if !is_contact && !is_battery {
            continue;
        }
        for characteristic in service.get("characteristics")?.as_array()? {
            let ty = characteristic.get("type")?;
            if is_contact && hap_type(ty, "6A", "0000006A-0000-1000-8000-0026BB765291") {
                if contact.is_some() {
                    return None;
                }
                contact = Some(match characteristic.get("value")?.as_u64()? {
                    0 => false,
                    1 => true,
                    _ => return None,
                });
            }
            if is_battery && hap_type(ty, "68", "00000068-0000-1000-8000-0026BB765291") {
                if battery.is_some() {
                    return None;
                }
                battery = Some(
                    characteristic
                        .get("value")?
                        .as_u64()
                        .filter(|v| *v <= 100)?,
                );
            }
        }
    }
    let mut state = Map::new();
    state.insert("open".into(), json!(contact?));
    if let Some(value) = battery {
        state.insert("battery_pct".into(), json!(value));
    }
    Some(Value::Object(state))
}

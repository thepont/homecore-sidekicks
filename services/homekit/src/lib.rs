use serde_json::{json, Map, Value};

struct PairingTemp(std::path::PathBuf);
impl Drop for PairingTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

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

/// Pair a HomeKit contact accessory and read its initial state.
pub async fn pair_and_read(
    accessory: hap_controller::Discovered,
    pin: &str,
    state_dir: &std::path::Path,
) -> Result<Value, &'static str> {
    use hap_controller::{
        CharacteristicType, Discovered, HapController, JsonFileStore, PairingStore, ServiceType,
    };
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let Discovered::Ip(ref ip) = accessory;
    let address = ip.addr;
    let id = ip.id.as_str();
    let allowed_address = match address.ip() {
        std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_loopback(),
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback() || ip.is_unicast_link_local() || ip.is_unique_local()
        }
    };
    if !allowed_address
        || address.port() == 0
        || id.len() != 17
        || !id.bytes().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        || pin.len() != 10
        || !pin.bytes().enumerate().all(|(i, b)| {
            if i == 3 || i == 6 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err("Invalid pairing details");
    }
    let metadata = std::fs::symlink_metadata(state_dir).map_err(|_| "Invalid pairing directory")?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Invalid pairing directory");
    }
    let lock = std::fs::File::open(state_dir).map_err(|_| "Invalid pairing directory")?;
    lock.try_lock().map_err(|_| "Pairing already in progress")?;
    let path = state_dir.join("pairing.json");
    if let Ok(m) = std::fs::symlink_metadata(&path) {
        if !m.is_file() || m.permissions().mode() & 0o077 != 0 {
            return Err("Invalid pairing store");
        }
    }
    let tmp = path.with_extension("json.tmp");
    if let Ok(metadata) = std::fs::symlink_metadata(&tmp) {
        if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("Invalid pairing temporary file");
        }
        std::fs::remove_file(&tmp).map_err(|_| "Cannot secure pairing store")?;
    }
    let create_tmp = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map(|_| ())
            .map_err(|_| "Cannot secure pairing store")
    };
    create_tmp()?;
    let _temporary = PairingTemp(tmp.clone());
    let store = JsonFileStore::new(&path);
    if store
        .load_controller()
        .await
        .map_err(|_| "Cannot read HomeKit identity")?
        .is_none()
    {
        let mut key = hap_crypto::ControllerKeypair::generate(String::new());
        let public = key.ltpk();
        key.id = format!(
            "homecore-{}",
            public[..16]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        store
            .save_controller(&key)
            .await
            .map_err(|_| "Cannot save HomeKit identity")?;
        create_tmp()?;
    }
    let mut controller = HapController::new(store)
        .await
        .map_err(|_| "Cannot initialize HomeKit controller")?;
    if !tmp.exists() {
        create_tmp()?;
    }
    let mut handle = controller
        .pair(&accessory, pin)
        .await
        .map_err(|_| "HomeKit pairing failed")?;
    handle
        .accessories()
        .await
        .map_err(|_| "Cannot read HomeKit accessories")?;
    let (aid, iid) = handle
        .find(
            ServiceType::ContactSensor,
            CharacteristicType::ContactSensorState,
        )
        .map_err(|_| "Contact sensor unavailable")?;
    let contact = handle
        .read(aid, iid)
        .await
        .map_err(|_| "Cannot read contact")?
        .to_json();
    let mut services = vec![json!({"type":"80","characteristics":[{"type":"6A","value":contact}]})];
    if let Ok((battery_aid, battery_iid)) =
        handle.find(ServiceType::Battery, CharacteristicType::BatteryLevel)
    {
        if battery_aid == aid {
            let battery = handle
                .read(battery_aid, battery_iid)
                .await
                .map_err(|_| "Cannot read battery")?
                .to_json();
            services.push(json!({"type":"96","characteristics":[{"type":"68","value":battery}]}));
        }
    }
    contact_state(&json!({"services":services})).ok_or("Invalid contact reading")
}

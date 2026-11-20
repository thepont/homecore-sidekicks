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
        Discovered, HapController, JsonFileStore, PairingStore, StoredAccessory, StoredTransport,
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
    if !tmp.exists() {
        create_tmp()?;
    }
    let key = store
        .load_controller()
        .await
        .map_err(|_| "Cannot read HomeKit identity")?
        .ok_or("HomeKit identity unavailable")?;
    let mut connection = hap_transport::HapConnection::connect(address)
        .await
        .map_err(|_| "Cannot connect to HomeKit accessory")?;
    let pairing = pair_setup(&mut connection, pin, &key).await?;
    drop(connection);
    let valid_id = pairing.pairing_id.eq_ignore_ascii_case(id);
    let paired_id = pairing.pairing_id.clone();
    let saved = StoredAccessory {
        pairing,
        transport: StoredTransport::Ip { addr: address },
    };
    if !valid_id || store.save_pairing(&saved).await.is_err() {
        let rollback = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut session = hap_pairing::connect(&saved, &key).await?;
            hap_pairing::PairingsAdmin::new(&mut session)
                .remove(&key.id)
                .await
        })
        .await;
        return match rollback {
            Ok(Ok(())) => Err("Pairing could not be saved; the temporary pairing was removed"),
            _ => Err("Pairing could not be saved or removed; manual recovery is needed"),
        };
    }
    let controller = HapController::new(store)
        .await
        .map_err(|_| "Cannot initialize HomeKit controller")?;
    let mut handle = controller
        .connect(&paired_id)
        .await
        .map_err(|_| "Cannot reconnect to HomeKit accessory")?;
    read_contact(&mut handle).await
}

/// Reconnect to a previously paired contact accessory.
pub async fn read_saved(id: &str, state_dir: &std::path::Path) -> Result<Value, &'static str> {
    let (_lock, mut handle) = connect_saved(id, state_dir).await?;
    read_contact(&mut handle).await
}

/// Read each bridged contact independently, keyed by its accessory ID.
pub async fn read_contacts_saved(
    id: &str,
    state_dir: &std::path::Path,
) -> Result<Vec<(u64, Value)>, &'static str> {
    use hap_controller::{CharacteristicType, ServiceType};
    let (_lock, mut handle) = connect_saved(id, state_dir).await?;
    let accessories = handle
        .accessories()
        .await
        .map_err(|_| "Cannot read HomeKit accessories")?
        .to_vec();
    let mut readings = Vec::new();
    for accessory in accessories {
        let mut contact = None;
        let mut battery = None;
        for service in &accessory.services {
            for characteristic in &service.characteristics {
                if service.service_type == ServiceType::ContactSensor
                    && characteristic.char_type == CharacteristicType::ContactSensorState
                {
                    if contact.replace(characteristic.iid).is_some() {
                        return Err("Ambiguous contact sensor");
                    }
                }
                if service.service_type == ServiceType::Battery
                    && characteristic.char_type == CharacteristicType::BatteryLevel
                {
                    if battery.replace(characteristic.iid).is_some() {
                        return Err("Ambiguous battery sensor");
                    }
                }
            }
        }
        if let Some(iid) = contact {
            let value = handle
                .read(accessory.aid, iid)
                .await
                .map_err(|_| "Cannot read contact")?
                .to_json();
            let mut services =
                vec![json!({"type":"80","characteristics":[{"type":"6A","value":value}]})];
            if let Some(iid) = battery {
                let value = handle
                    .read(accessory.aid, iid)
                    .await
                    .map_err(|_| "Cannot read battery")?
                    .to_json();
                services.push(json!({"type":"96","characteristics":[{"type":"68","value":value}]}));
            }
            let state =
                contact_state(&json!({"services":services})).ok_or("Invalid contact reading")?;
            readings.push((accessory.aid, state));
        }
    }
    Ok(readings)
}

async fn connect_saved(
    id: &str,
    state_dir: &std::path::Path,
) -> Result<(std::fs::File, hap_controller::AccessoryHandle), &'static str> {
    use hap_controller::{HapController, JsonFileStore, PairingStore};
    use std::os::unix::fs::PermissionsExt;
    if id.len() != 17
        || !id.bytes().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("Invalid pairing identity");
    }
    let metadata = std::fs::symlink_metadata(state_dir).map_err(|_| "Invalid pairing directory")?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Invalid pairing directory");
    }
    let lock = std::fs::File::open(state_dir).map_err(|_| "Invalid pairing directory")?;
    lock.try_lock().map_err(|_| "Pairing already in progress")?;
    let path = state_dir.join("pairing.json");
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "Pairing store unavailable")?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > 1024 * 1024
    {
        return Err("Invalid pairing store");
    }
    let store = JsonFileStore::new(path);
    if store
        .load_controller()
        .await
        .map_err(|_| "Cannot read HomeKit identity")?
        .is_none()
    {
        return Err("HomeKit identity unavailable");
    }
    let controller = HapController::new(store)
        .await
        .map_err(|_| "Cannot initialize HomeKit controller")?;
    let handle = controller
        .connect(id)
        .await
        .map_err(|_| "Cannot reconnect to HomeKit accessory")?;
    Ok((lock, handle))
}

async fn read_contact(handle: &mut hap_controller::AccessoryHandle) -> Result<Value, &'static str> {
    use hap_controller::{CharacteristicType, ServiceType};
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

// Drive the library's Pair Setup state machine separately from Pair Verify so
// the verified accessory keys can be saved before opening another session.
async fn pair_setup(
    connection: &mut hap_transport::HapConnection,
    pin: &str,
    key: &hap_crypto::ControllerKeypair,
) -> Result<hap_crypto::AccessoryPairing, &'static str> {
    use hap_crypto::{PairSetupClient, PairSetupStep};
    let mut client =
        PairSetupClient::new(pin, key.clone()).map_err(|_| "HomeKit pairing failed")?;
    let mut request = client.start();
    for step in 0..3 {
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            connection.request("POST", "/pair-setup", "application/pairing+tlv8", &request),
        )
        .await;
        let response = match response {
            Ok(Ok(response)) => response,
            _ if step == 2 => {
                return Err("Pairing confirmation was lost; accessory recovery may be needed")
            }
            _ => return Err("HomeKit pairing failed"),
        };
        if response.status != 200 {
            return Err("HomeKit pairing failed");
        }
        match client
            .handle(&response.body)
            .map_err(|_| "HomeKit pairing failed")?
        {
            PairSetupStep::Send(next) => request = next,
            PairSetupStep::Done(pairing) => return Ok(pairing),
        }
    }
    Err("HomeKit pairing failed")
}

pub struct ContactWatch {
    current: Value,
    updates: tokio::sync::mpsc::Receiver<Value>,
    task: tokio::task::JoinHandle<()>,
}
impl ContactWatch {
    pub fn current(&self) -> &Value {
        &self.current
    }
    pub async fn next(&mut self) -> Option<Value> {
        let state = self.updates.recv().await?;
        self.current = state.clone();
        Some(state)
    }
}
impl Drop for ContactWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}
/// Start receiving contact changes from a previously paired accessory.
pub async fn watch_saved(
    id: &str,
    state_dir: &std::path::Path,
) -> Result<ContactWatch, &'static str> {
    use hap_controller::{CharacteristicType, ServiceType};
    use tokio_stream::StreamExt;
    let (lock, mut handle) = connect_saved(id, state_dir).await?;
    let current = read_contact(&mut handle).await?;
    let (aid, iid) = handle
        .find(
            ServiceType::ContactSensor,
            CharacteristicType::ContactSensorState,
        )
        .map_err(|_| "Contact sensor unavailable")?;
    let mut events = Box::pin(handle.events());
    handle
        .subscribe(aid, iid)
        .await
        .map_err(|_| "Cannot subscribe to contact")?;
    let (send, updates) = tokio::sync::mpsc::channel(32);
    let mut state = current.clone();
    let task = tokio::spawn(async move {
        let _lock = lock;
        let _handle = handle;
        while let Some(event) = events.next().await {
            if event.aid != aid || event.iid != iid {
                continue;
            }
            let open = match event.value.to_json().as_u64() {
                Some(0) => false,
                Some(1) => true,
                _ => continue,
            };
            state["open"] = json!(open);
            if send.send(state.clone()).await.is_err() {
                break;
            }
        }
    });
    Ok(ContactWatch {
        current,
        updates,
        task,
    })
}

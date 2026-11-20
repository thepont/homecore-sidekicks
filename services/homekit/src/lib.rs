use serde_json::{json, Map, Value};
pub mod setup;

/// Publish a validated contact snapshot using homeCore's device protocol.
pub async fn publish_contacts(
    mqtt: &rumqttc::AsyncClient,
    plugin: &str,
    bridge_id: &str,
    readings: &[(u64, Value)],
) -> Result<(), &'static str> {
    if plugin.is_empty()
        || plugin.len() > 128
        || !plugin
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        || bridge_id.len() != 17
        || !bridge_id.bytes().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        || readings.len() > 1024
    {
        return Err("Invalid contact publication");
    }
    let bridge = bridge_id.replace(':', "").to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    for (aid, value) in readings {
        let object = value.as_object().ok_or("Invalid contact state")?;
        if *aid == 0
            || !seen.insert(*aid)
            || object.len() > 2
            || object.get("open").and_then(Value::as_bool).is_none()
            || object
                .keys()
                .any(|key| key != "open" && key != "battery_pct")
            || object
                .get("battery_pct")
                .is_some_and(|battery| battery.as_u64().is_none_or(|n| n > 100))
        {
            return Err("Invalid contact state");
        }
    }
    for (aid, state) in readings {
        let device = format!("homekit_{bridge}_{aid}");
        let mut attributes = Map::new();
        attributes.insert(
            "open".into(),
            json!({"kind":"bool", "writable":false, "display_name":"Open"}),
        );
        if state.get("battery_pct").is_some() {
            attributes.insert(
                "battery_pct".into(),
                json!({"kind":"integer", "writable":false, "display_name":"Battery", "unit":"%"}),
            );
        }
        let register = json!({"device_id":device,"plugin_id":plugin,"name":format!("HomeKit contact {aid}"),"device_type":"contact_sensor"});
        for (topic, retained, payload) in [
            (
                format!("homecore/plugins/{plugin}/register"),
                false,
                register,
            ),
            (
                format!("homecore/devices/{device}/schema"),
                true,
                json!({"attributes":attributes,"primary":["open"]}),
            ),
            (
                format!("homecore/devices/{device}/state"),
                true,
                state.clone(),
            ),
        ] {
            mqtt.publish(
                topic,
                rumqttc::QoS::AtLeastOnce,
                retained,
                payload.to_string(),
            )
            .await
            .map_err(|_| "Cannot publish contact")?;
        }
    }
    Ok(())
}

/// Publish a saved bridge's initial snapshot and subsequent contact changes.
pub async fn run_contact_publisher(
    mqtt: rumqttc::AsyncClient,
    plugin: String,
    bridge_id: String,
    state_dir: std::path::PathBuf,
) -> Result<(), &'static str> {
    run_contact_publisher_with_interval(
        mqtt,
        plugin,
        bridge_id,
        state_dir,
        std::time::Duration::from_secs(30),
    )
    .await
}

pub async fn run_contact_publisher_with_interval(
    mqtt: rumqttc::AsyncClient,
    plugin: String,
    bridge_id: String,
    state_dir: std::path::PathBuf,
    refresh: std::time::Duration,
) -> Result<(), &'static str> {
    if refresh < std::time::Duration::from_secs(1) || refresh > std::time::Duration::from_secs(3600)
    {
        return Err("Invalid refresh interval");
    }
    let mut watch = watch_contacts_with_interval(&bridge_id, &state_dir, refresh).await?;
    publish_contacts(&mqtt, &plugin, &bridge_id, watch.current()).await?;
    contact_availability(&mqtt, &bridge_id, watch.current(), true).await?;
    while watch.next().await.is_some() {
        publish_contacts(&mqtt, &plugin, &bridge_id, watch.current()).await?;
    }
    contact_availability(&mqtt, &bridge_id, watch.current(), false).await?;
    Err("HomeKit contact stream stopped")
}

async fn contact_availability(
    mqtt: &rumqttc::AsyncClient,
    bridge_id: &str,
    readings: &[(u64, Value)],
    online: bool,
) -> Result<(), &'static str> {
    let bridge = bridge_id.replace(':', "").to_ascii_lowercase();
    for (aid, _) in readings {
        mqtt.publish(
            format!("homecore/devices/homekit_{bridge}_{aid}/availability"),
            rumqttc::QoS::AtLeastOnce,
            true,
            if online { "online" } else { "offline" },
        )
        .await
        .map_err(|_| "Cannot publish availability")?;
    }
    Ok(())
}

struct PairingTemp(std::path::PathBuf);
impl Drop for PairingTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct PairingAttempt {
    path: std::path::PathBuf,
    file: std::fs::File,
}
impl PairingAttempt {
    fn clear(mut self) -> Result<(), &'static str> {
        use std::io::{Seek, Write};
        if std::fs::remove_file(&self.path).is_ok() {
            return Ok(());
        }
        self.file
            .set_len(0)
            .and_then(|_| self.file.rewind())
            .and_then(|_| self.file.write_all(b"safe"))
            .and_then(|_| self.file.sync_all())
            .map_err(|_| "Cannot clear pairing recovery state")
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
    let recovery_marker = state_dir.join("pairing-recovery-required");
    match std::fs::symlink_metadata(&recovery_marker) {
        Ok(metadata)
            if metadata.is_file()
                && metadata.len() == 4
                && std::fs::read(&recovery_marker).ok().as_deref() == Some(b"safe") =>
        {
            std::fs::remove_file(&recovery_marker)
                .map_err(|_| "Cannot clear pairing recovery state")?;
        }
        Ok(_) => return Err("Pairing requires recovery before retrying"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Cannot check pairing recovery state"),
    }
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
    if !store
        .load_pairings()
        .await
        .map_err(|_| "Cannot read pairing")?
        .is_empty()
    {
        return Err("Controller already configured");
    }
    if store
        .load_controller()
        .await
        .map_err(|_| "Cannot read HomeKit identity")?
        .is_none()
    {
        let key = hap_crypto::ControllerKeypair::generate(uuid::Uuid::new_v4().to_string());
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
    let mut marker_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&recovery_marker)
        .map_err(|_| "Cannot secure pairing recovery state")?;
    use std::io::Write;
    marker_file
        .write_all(b"pending")
        .and_then(|_| marker_file.sync_all())
        .map_err(|_| "Cannot secure pairing recovery state")?;
    let attempt = PairingAttempt {
        path: recovery_marker,
        file: marker_file,
    };
    let mut connection = match hap_transport::HapConnection::connect(address).await {
        Ok(connection) => connection,
        Err(_) => {
            attempt.clear()?;
            return Err("Cannot connect to HomeKit accessory");
        }
    };
    let pairing = match pair_setup(&mut connection, pin, &key).await {
        Ok(pairing) => pairing,
        Err(error) if error.contains("confirmation was lost") => return Err(error),
        Err(error) => {
            attempt.clear()?;
            return Err(error);
        }
    };
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
            Ok(Ok(())) => {
                attempt.clear()?;
                Err("Pairing could not be saved; the temporary pairing was removed")
            }
            _ => Err("Pairing could not be saved or removed; manual recovery is needed"),
        };
    }
    attempt.clear()?;
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

/// Return the one stored bridge identifier, if setup has completed.
pub async fn stored_bridge_id(state_dir: &std::path::Path) -> Result<Option<String>, &'static str> {
    use hap_controller::{JsonFileStore, PairingStore};
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::symlink_metadata(state_dir).map_err(|_| "Invalid pairing directory")?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Invalid pairing directory");
    }
    let lock = std::fs::File::open(state_dir).map_err(|_| "Invalid pairing directory")?;
    lock.try_lock().map_err(|_| "Pairing already in progress")?;
    let path = state_dir.join("pairing.json");
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Invalid pairing store"),
    };
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > 1024 * 1024
    {
        return Err("Invalid pairing store");
    }
    let pairings = JsonFileStore::new(path)
        .load_pairings()
        .await
        .map_err(|_| "Cannot read pairing")?;
    match pairings.as_slice() {
        [] => Ok(None),
        [one] => {
            let id = &one.pairing.pairing_id;
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
            Ok(Some(id.clone()))
        }
        _ => Err("Ambiguous pairing store"),
    }
}

/// Invite a separate, non-admin controller using an existing administrator pairing.
pub async fn invite_controller(
    id: &str,
    admin_dir: &std::path::Path,
    target_dir: &std::path::Path,
) -> Result<(), &'static str> {
    use hap_controller::{JsonFileStore, PairingStore};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let (_admin_lock, admin_handle) = connect_saved(id, admin_dir).await?;
    drop(admin_handle);
    let admin_meta = std::fs::metadata(admin_dir).map_err(|_| "Invalid pairing directory")?;
    let meta = std::fs::symlink_metadata(target_dir).map_err(|_| "Invalid target directory")?;
    if !meta.is_dir()
        || meta.permissions().mode() & 0o077 != 0
        || (meta.dev() == admin_meta.dev() && meta.ino() == admin_meta.ino())
    {
        return Err("Invalid target directory");
    }
    let target_lock = std::fs::File::open(target_dir).map_err(|_| "Invalid target directory")?;
    target_lock
        .try_lock()
        .map_err(|_| "Target pairing in progress")?;
    let path = target_dir.join("pairing.json");
    let temp = path.with_extension("json.tmp");
    if path.exists() {
        return Err("Target pairing already exists");
    }
    let create_tmp = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map(|_| ())
            .map_err(|_| "Cannot secure target pairing store")
    };
    create_tmp()?;
    let _temporary = PairingTemp(temp.clone());
    let admin_store = JsonFileStore::new(admin_dir.join("pairing.json"));
    let admin_key = admin_store
        .load_controller()
        .await
        .map_err(|_| "Cannot read HomeKit identity")?
        .ok_or("HomeKit identity unavailable")?;
    let pairing = admin_store
        .load_pairings()
        .await
        .map_err(|_| "Cannot read pairing")?
        .into_iter()
        .find(|p| p.pairing.pairing_id.eq_ignore_ascii_case(id))
        .ok_or("Pairing unavailable")?;
    let key = hap_crypto::ControllerKeypair::generate(uuid::Uuid::new_v4().to_string());
    let target = JsonFileStore::new(&path);
    target
        .save_controller(&key)
        .await
        .map_err(|_| "Cannot save target identity")?;
    create_tmp()?;
    target
        .save_pairing(&pairing)
        .await
        .map_err(|_| "Cannot save target pairing")?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut session = hap_pairing::connect(&pairing, &admin_key).await?;
        hap_pairing::PairingsAdmin::new(&mut session)
            .add(&key.id, key.ltpk(), false)
            .await
    })
    .await;
    if !matches!(result, Ok(Ok(()))) {
        return Err(
            "Invitation failed or its confirmation was lost; target credentials were retained",
        );
    }
    Ok(())
}

/// Read each bridged contact independently, keyed by its accessory ID.
pub async fn read_contacts_saved(
    id: &str,
    state_dir: &std::path::Path,
) -> Result<Vec<(u64, Value)>, &'static str> {
    let (_lock, mut handle) = connect_saved(id, state_dir).await?;
    read_contacts(&mut handle).await
}

async fn read_contacts(
    handle: &mut hap_controller::AccessoryHandle,
) -> Result<Vec<(u64, Value)>, &'static str> {
    use hap_controller::{CharacteristicType, ServiceType};
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
                    && contact.replace(characteristic.iid).is_some()
                {
                    return Err("Ambiguous contact sensor");
                }
                if service.service_type == ServiceType::Battery
                    && characteristic.char_type == CharacteristicType::BatteryLevel
                    && battery.replace(characteristic.iid).is_some()
                {
                    return Err("Ambiguous battery sensor");
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
            return Err(if step == 2 {
                "Pairing confirmation was lost; accessory recovery may be needed"
            } else {
                "HomeKit pairing failed"
            });
        }
        match client.handle(&response.body).map_err(|_| {
            if step == 2 {
                "Pairing confirmation was lost; accessory recovery may be needed"
            } else {
                "HomeKit pairing failed"
            }
        })? {
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

pub struct ContactsWatch {
    current: Vec<(u64, Value)>,
    updates: tokio::sync::mpsc::Receiver<(u64, Value)>,
    task: tokio::task::JoinHandle<()>,
}
impl ContactsWatch {
    pub fn current(&self) -> &Vec<(u64, Value)> {
        &self.current
    }
    pub async fn next(&mut self) -> Option<(u64, Value)> {
        let update = self.updates.recv().await?;
        if let Some((_, state)) = self.current.iter_mut().find(|(aid, _)| *aid == update.0) {
            *state = update.1.clone();
        }
        Some(update)
    }
}
impl Drop for ContactsWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Watch contact notifications for all contacts attached to a saved bridge.
pub async fn watch_contacts_saved(
    id: &str,
    state_dir: &std::path::Path,
) -> Result<ContactsWatch, &'static str> {
    watch_contacts_with_interval(id, state_dir, std::time::Duration::from_secs(30)).await
}

async fn watch_contacts_with_interval(
    id: &str,
    state_dir: &std::path::Path,
    refresh: std::time::Duration,
) -> Result<ContactsWatch, &'static str> {
    use hap_controller::{CharacteristicType, ServiceType};
    use tokio_stream::StreamExt;
    let (lock, mut handle) = connect_saved(id, state_dir).await?;
    let accessories = handle
        .accessories()
        .await
        .map_err(|_| "Cannot read HomeKit accessories")?
        .to_vec();
    let mut contacts = Vec::new();
    let mut batteries = Vec::new();
    for accessory in accessories {
        for service in accessory.services {
            for characteristic in service.characteristics {
                if service.service_type == ServiceType::ContactSensor
                    && characteristic.char_type == CharacteristicType::ContactSensorState
                {
                    contacts.push((accessory.aid, characteristic.iid));
                } else if service.service_type == ServiceType::Battery
                    && characteristic.char_type == CharacteristicType::BatteryLevel
                    && characteristic.perms.events
                {
                    batteries.push((accessory.aid, characteristic.iid));
                }
            }
        }
    }
    batteries.retain(|(aid, _)| contacts.iter().any(|(contact_aid, _)| contact_aid == aid));
    let mut events = Box::pin(handle.events());
    for (aid, iid) in contacts.iter().chain(&batteries) {
        handle
            .subscribe(*aid, *iid)
            .await
            .map_err(|_| "Cannot subscribe to contact")?;
    }
    let current = read_contacts(&mut handle).await?;
    let (send, updates) = tokio::sync::mpsc::channel(32);
    let mut state = current.clone();
    let task = tokio::spawn(async move {
        let _lock = lock;
        let mut timer = tokio::time::interval(refresh);
        timer.tick().await;
        loop {
            let event = tokio::select! {
                event = events.next() => match event { Some(event) => event, None => break },
                _ = timer.tick() => {
                    let refreshed = match tokio::time::timeout(std::time::Duration::from_secs(3), read_contacts(&mut handle)).await {
                        Ok(Ok(refreshed)) => refreshed,
                        _ => break,
                    };
                    for (aid, value) in &refreshed {
                        if state.iter().find(|(old, _)| old == aid).is_none_or(|(_, old)| old != value) && send.try_send((*aid, value.clone())).is_err() { return; }
                    }
                    state = refreshed;
                    continue;
                }
            };
            let is_contact = contacts.contains(&(event.aid, event.iid));
            if !is_contact && !batteries.contains(&(event.aid, event.iid)) {
                continue;
            }
            let value_at_event = match handle.read(event.aid, event.iid).await {
                Ok(value) => value.to_json(),
                Err(_) => break,
            };
            if let Some((_, value)) = state.iter_mut().find(|(aid, _)| *aid == event.aid) {
                if is_contact {
                    value["open"] = match value_at_event.as_u64() {
                        Some(0) => json!(false),
                        Some(1) => json!(true),
                        _ => continue,
                    };
                } else {
                    value["battery_pct"] = match value_at_event.as_u64() {
                        Some(n) if n <= 100 => json!(n),
                        _ => continue,
                    };
                }
                if send.try_send((event.aid, value.clone())).is_err() {
                    break;
                }
            }
        }
    });
    Ok(ContactsWatch {
        current,
        updates,
        task,
    })
}

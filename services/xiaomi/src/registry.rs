use crate::adapters::{valid_identifier, valid_plugin_id, AdapterCatalog, AdapterRegistration};
use crate::vault::{valid_device_ref, StoredDevice, Vault};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[cfg(feature = "postgres")]
mod postgres;

const MAX_BYTES: usize = 65536;
const MAX_REQUESTS: usize = 64;
const MAX_DEVICES: usize = 64;
const MAX_GRANTS: usize = 128;
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS xiaomi_adapter_registry (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        registrations TEXT NOT NULL CHECK (length(registrations) <= 65536)
    )";
const SELECT: &str = "SELECT registrations FROM xiaomi_adapter_registry WHERE id = 1";
const SAVE: &str = "
    INSERT INTO xiaomi_adapter_registry (id, registrations) VALUES (1, $1)
    ON CONFLICT (id) DO UPDATE SET registrations = excluded.registrations
    WHERE xiaomi_adapter_registry.registrations <> excluded.registrations";
const DATABASE_ERROR: &str = "Cannot update the Xiaomi adapter database";

pub(crate) enum Registry {
    Sqlite(PathBuf),
    #[cfg(feature = "postgres")]
    Postgres(Box<postgres::Postgres>),
}

pub(crate) enum Operation {
    Bootstrap(Option<AdapterCatalog>),
    Register(AdapterRegistration),
    Request(AccessRequest),
    Remember(Vec<StoredDevice>),
    Approve(DeviceGrant, Arc<Vault>),
    Deny(AccessRequest),
    Revoke(DeviceGrant),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AccessRequest {
    plugin_id: String,
    model: String,
}

impl AccessRequest {
    pub(crate) fn parse(value: &Value) -> Result<Self, &'static str> {
        let plugin_id = value["plugin_id"]
            .as_str()
            .filter(|id| valid_plugin_id(id))
            .ok_or("Invalid requesting adapter")?;
        let model = value["model"]
            .as_str()
            .filter(|model| valid_identifier(model))
            .ok_or("Invalid requested model")?;
        Ok(Self {
            plugin_id: plugin_id.to_owned(),
            model: model.to_owned(),
        })
    }

    fn json(&self) -> Value {
        json!({"plugin_id": self.plugin_id, "model": self.model, "status": "pending"})
    }

    fn supported_by(&self, catalog: &AdapterCatalog) -> bool {
        catalog.supports(&self.plugin_id, &self.model)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DeviceGrant {
    plugin_id: String,
    device_ref: String,
}

impl DeviceGrant {
    pub(crate) fn parse(value: &Value) -> Result<Self, &'static str> {
        let plugin_id = value["plugin_id"]
            .as_str()
            .filter(|id| valid_plugin_id(id))
            .ok_or("Invalid requesting adapter")?;
        let device_ref = value["device_ref"]
            .as_str()
            .filter(|id| valid_device_ref(id))
            .ok_or("Invalid Xiaomi device reference")?;
        Ok(Self {
            plugin_id: plugin_id.to_owned(),
            device_ref: device_ref.to_owned(),
        })
    }

    fn document(&self) -> Value {
        json!({"plugin_id":self.plugin_id,"device_ref":self.device_ref})
    }
}

#[derive(Default)]
pub(crate) struct RegistryState {
    pub(crate) catalog: Option<AdapterCatalog>,
    requests: BTreeSet<AccessRequest>,
    devices: BTreeMap<String, StoredDevice>,
    grants: BTreeSet<DeviceGrant>,
}

impl RegistryState {
    pub(crate) fn pending_requests(&self) -> Value {
        Value::Array(self.requests.iter().map(AccessRequest::json).collect())
    }

    pub(crate) fn has_pending_requests(&self) -> bool {
        !self.requests.is_empty()
    }

    pub(crate) fn known_devices(&self) -> Value {
        let Some(catalog) = &self.catalog else {
            return json!([]);
        };
        Value::Array(
            self.devices
                .values()
                .map(|device| {
                    let adapters = catalog.adapters(&device.model);
                    let grants: Vec<_> = self
                        .grants
                        .iter()
                        .filter(|grant| grant.device_ref == device.reference)
                        .map(|grant| &grant.plugin_id)
                        .collect();
                    json!({
                        "device_ref": device.reference, "name": device.name, "model": device.model,
                        "adapters": adapters, "granted_adapters": grants,
                    })
                })
                .collect(),
        )
    }

    fn from_document(document: &str) -> Result<Self, &'static str> {
        if document.len() > MAX_BYTES {
            return Err(DATABASE_ERROR);
        }
        let data: Value = serde_json::from_str(document).map_err(|_| DATABASE_ERROR)?;
        if data.is_array() {
            return Ok(Self {
                catalog: Some(AdapterCatalog::parse(&data)?),
                ..Self::default()
            });
        }
        if data["version"].as_u64() != Some(1) {
            return Err(DATABASE_ERROR);
        }
        let catalog = match data.get("adapters") {
            Some(Value::Null) => None,
            Some(adapters) => Some(AdapterCatalog::parse(adapters)?),
            None => return Err(DATABASE_ERROR),
        };
        let requests = Self::read_requests(&data["requests"], catalog.as_ref())?;
        let devices = Self::read_devices(optional_entries(&data, "devices", MAX_DEVICES)?)?;
        let grants = Self::read_grants(
            optional_entries(&data, "grants", MAX_GRANTS)?,
            catalog.as_ref(),
            &devices,
        )?;
        Ok(Self {
            catalog,
            requests,
            devices,
            grants,
        })
    }

    fn read_requests(
        data: &Value,
        catalog: Option<&AdapterCatalog>,
    ) -> Result<BTreeSet<AccessRequest>, &'static str> {
        let stored = data
            .as_array()
            .filter(|entries| entries.len() <= MAX_REQUESTS)
            .ok_or(DATABASE_ERROR)?;
        let mut requests = BTreeSet::new();
        for entry in stored {
            let request = AccessRequest::parse(entry)?;
            if !catalog.is_some_and(|catalog| request.supported_by(catalog))
                || !requests.insert(request)
            {
                return Err(DATABASE_ERROR);
            }
        }
        Ok(requests)
    }

    fn read_devices(entries: &[Value]) -> Result<BTreeMap<String, StoredDevice>, &'static str> {
        let mut devices = BTreeMap::new();
        for entry in entries {
            let device = StoredDevice::parse(entry)?;
            if devices.insert(device.reference.clone(), device).is_some() {
                return Err(DATABASE_ERROR);
            }
        }
        Ok(devices)
    }

    fn read_grants(
        entries: &[Value],
        catalog: Option<&AdapterCatalog>,
        devices: &BTreeMap<String, StoredDevice>,
    ) -> Result<BTreeSet<DeviceGrant>, &'static str> {
        let mut grants = BTreeSet::new();
        for entry in entries {
            let grant = DeviceGrant::parse(entry)?;
            let device = devices.get(&grant.device_ref).ok_or(DATABASE_ERROR)?;
            if !catalog.is_some_and(|catalog| catalog.supports(&grant.plugin_id, &device.model))
                || !grants.insert(grant)
            {
                return Err(DATABASE_ERROR);
            }
        }
        Ok(grants)
    }

    fn document(&self) -> Result<String, &'static str> {
        let payload = json!({
            "version": 1,
            "adapters": self.catalog.as_ref().map(AdapterCatalog::registrations),
            "requests": self.pending_requests(),
            "devices": self.devices.values().map(StoredDevice::document).collect::<Vec<_>>(),
            "grants": self.grants.iter().map(DeviceGrant::document).collect::<Vec<_>>(),
        })
        .to_string();
        if payload.len() > MAX_BYTES {
            return Err("Xiaomi adapter database is full");
        }
        Ok(payload)
    }

    fn apply(
        document: Option<&str>,
        action: Operation,
    ) -> Result<(Self, Option<String>), &'static str> {
        let mut state = document
            .map(Self::from_document)
            .transpose()?
            .unwrap_or_default();
        let changed = match action {
            Operation::Bootstrap(import) => {
                if state.catalog.is_some() || (document.is_some() && import.is_none()) {
                    return Ok((state, None));
                }
                state.catalog = import;
                true
            }
            Operation::Register(registration) => {
                let catalog = state.catalog.get_or_insert_with(AdapterCatalog::default);
                catalog.upsert(registration)?;
                if state
                    .requests
                    .iter()
                    .any(|request| !request.supported_by(catalog))
                {
                    return Err("Adapter still has a pending request for a removed model");
                }
                if state.grants.iter().any(|grant| {
                    !state
                        .devices
                        .get(&grant.device_ref)
                        .is_some_and(|device| catalog.supports(&grant.plugin_id, &device.model))
                }) {
                    return Err("Adapter still has access to a removed model");
                }
                true
            }
            Operation::Request(request) => state.queue_request(request)?,
            Operation::Remember(devices) => {
                for device in devices {
                    if state
                        .devices
                        .get(&device.reference)
                        .is_some_and(|old| old.model != device.model)
                    {
                        return Err("Conflicting Xiaomi device model");
                    }
                    state.devices.insert(device.reference.clone(), device);
                }
                if state.devices.len() > MAX_DEVICES {
                    return Err("Too many saved Xiaomi devices");
                }
                true
            }
            Operation::Approve(grant, vault) => state.approve_access(grant, &vault)?,
            Operation::Deny(request) => state.requests.remove(&request),
            Operation::Revoke(grant) => state.grants.remove(&grant),
        };
        let updated = changed.then(|| state.document()).transpose()?;
        Ok((state, updated))
    }

    fn queue_request(&mut self, request: AccessRequest) -> Result<bool, &'static str> {
        if !self
            .catalog
            .as_ref()
            .is_some_and(|catalog| request.supported_by(catalog))
        {
            return Err("Adapter does not support the requested model");
        }
        if self.requests.contains(&request)
            || self.grants.iter().any(|grant| {
                grant.plugin_id == request.plugin_id
                    && self
                        .devices
                        .get(&grant.device_ref)
                        .is_some_and(|device| device.model == request.model)
            })
        {
            return Ok(false);
        }
        if self.requests.len() >= MAX_REQUESTS {
            return Err("Too many pending Xiaomi access requests");
        }
        self.requests.insert(request);
        Ok(true)
    }

    fn approve_access(&mut self, grant: DeviceGrant, vault: &Vault) -> Result<bool, &'static str> {
        let device = self
            .devices
            .get(&grant.device_ref)
            .ok_or("Xiaomi device must be discovered first")?;
        vault.verify(device)?;
        if self.grants.contains(&grant) {
            return Ok(false);
        }
        let request = AccessRequest {
            plugin_id: grant.plugin_id.clone(),
            model: device.model.clone(),
        };
        if !self.requests.remove(&request) {
            return Err("No pending request for this Xiaomi device");
        }
        if self.grants.len() >= MAX_GRANTS {
            return Err("Too many Xiaomi access grants");
        }
        self.grants.insert(grant);
        Ok(true)
    }
}

fn optional_entries<'a>(
    value: &'a Value,
    key: &str,
    limit: usize,
) -> Result<&'a [Value], &'static str> {
    match value.get(key) {
        None => Ok(&[]),
        Some(entries) => entries
            .as_array()
            .filter(|items| items.len() <= limit)
            .map(Vec::as_slice)
            .ok_or(DATABASE_ERROR),
    }
}

impl Registry {
    #[cfg(feature = "postgres")]
    pub(crate) fn postgres(dsn: &Path, ca: Option<&Path>) -> Result<Self, &'static str> {
        Ok(Self::Postgres(Box::new(postgres::Postgres::new(dsn, ca)?)))
    }

    pub(crate) async fn run(&self, action: Operation) -> Result<RegistryState, &'static str> {
        match self {
            Self::Sqlite(path) => {
                let path = path.clone();
                tokio::task::spawn_blocking(move || sqlite(&path, action))
                    .await
                    .map_err(|_| DATABASE_ERROR)?
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(database) => database.run(action).await,
        }
    }
}

fn sqlite(path: &Path, action: Operation) -> Result<RegistryState, &'static str> {
    let mut connection = open_sqlite(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| DATABASE_ERROR)?;
    if matches!(action, Operation::Bootstrap(_)) {
        transaction
            .execute_batch(SCHEMA)
            .map_err(|_| DATABASE_ERROR)?;
    }
    let document: Option<String> = transaction
        .query_row(SELECT, [], |row| row.get(0))
        .optional()
        .map_err(|_| DATABASE_ERROR)?;
    let (state, updated) = RegistryState::apply(document.as_deref(), action)?;
    if let Some(document) = updated {
        transaction
            .execute(SAVE, [document])
            .map_err(|_| DATABASE_ERROR)?;
    }
    transaction.commit().map_err(|_| DATABASE_ERROR)?;
    Ok(state)
}

fn open_sqlite(path: &Path) -> Result<Connection, &'static str> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| DATABASE_ERROR)?;
    let metadata = file.metadata().map_err(|_| DATABASE_ERROR)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(DATABASE_ERROR);
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| DATABASE_ERROR)?;
    connection
        .busy_timeout(Duration::from_secs(3))
        .map_err(|_| DATABASE_ERROR)?;
    Ok(connection)
}

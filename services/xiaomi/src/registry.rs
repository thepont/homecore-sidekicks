use crate::adapters::{valid_identifier, valid_plugin_id, AdapterCatalog, AdapterRegistration};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(feature = "postgres")]
mod postgres;

const MAX_BYTES: usize = 65536;
const MAX_REQUESTS: usize = 64;
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

#[derive(Default)]
pub(crate) struct RegistryState {
    pub(crate) catalog: Option<AdapterCatalog>,
    requests: BTreeSet<AccessRequest>,
}

impl RegistryState {
    pub(crate) fn pending_requests(&self) -> Value {
        Value::Array(self.requests.iter().map(AccessRequest::json).collect())
    }

    fn from_document(document: &str) -> Result<Self, &'static str> {
        if document.len() > MAX_BYTES {
            return Err(DATABASE_ERROR);
        }
        let data: Value = serde_json::from_str(document).map_err(|_| DATABASE_ERROR)?;
        if data.is_array() {
            return Ok(Self {
                catalog: Some(AdapterCatalog::parse(&data)?),
                requests: BTreeSet::new(),
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
        let stored = data["requests"]
            .as_array()
            .filter(|entries| entries.len() <= MAX_REQUESTS)
            .ok_or(DATABASE_ERROR)?;
        let mut requests = BTreeSet::new();
        for entry in stored {
            let request = AccessRequest::parse(entry)?;
            if !catalog
                .as_ref()
                .is_some_and(|catalog| request.supported_by(catalog))
                || !requests.insert(request)
            {
                return Err(DATABASE_ERROR);
            }
        }
        Ok(Self { catalog, requests })
    }

    fn document(&self) -> Result<String, &'static str> {
        let payload = json!({
            "version": 1,
            "adapters": self.catalog.as_ref().map(AdapterCatalog::registrations),
            "requests": self.pending_requests(),
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
        match action {
            Operation::Bootstrap(import) => {
                if state.catalog.is_some() || (document.is_some() && import.is_none()) {
                    return Ok((state, None));
                }
                state.catalog = import;
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
            }
            Operation::Request(request) => {
                if !state
                    .catalog
                    .as_ref()
                    .is_some_and(|catalog| request.supported_by(catalog))
                {
                    return Err("Adapter does not support the requested model");
                }
                if state.requests.contains(&request) {
                    return Ok((state, None));
                }
                if state.requests.len() >= MAX_REQUESTS {
                    return Err("Too many pending Xiaomi access requests");
                }
                state.requests.insert(request);
            }
        }
        let updated = state.document()?;
        Ok((state, Some(updated)))
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

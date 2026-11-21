use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const MAX_CATALOG_BYTES: u64 = 64 * 1024;
const INVALID: &str = "Invalid Xiaomi adapter registration";

pub(crate) struct AdapterCatalog(BTreeMap<String, Vec<String>>);

impl AdapterCatalog {
    pub(crate) fn load(path: &Path) -> Result<Option<Self>, &'static str> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(INVALID),
        };
        let metadata = file.metadata().map_err(|_| INVALID)?;
        if !metadata.is_file() || metadata.len() > MAX_CATALOG_BYTES {
            return Err(INVALID);
        }
        let mut bytes = Vec::new();
        file.take(MAX_CATALOG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| INVALID)?;
        if bytes.len() as u64 > MAX_CATALOG_BYTES {
            return Err(INVALID);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
        Self::parse(&value).map(Some)
    }

    fn parse(value: &Value) -> Result<Self, &'static str> {
        let registrations = value
            .as_array()
            .filter(|xs| xs.len() <= 32)
            .ok_or(INVALID)?;
        let mut plugin_ids = BTreeSet::new();
        let mut by_model: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for registration in registrations {
            let plugin_id = registration["plugin_id"]
                .as_str()
                .filter(|id| id.starts_with("plugin.") && valid_identifier(id))
                .ok_or(INVALID)?;
            if !plugin_ids.insert(plugin_id) {
                return Err(INVALID);
            }
            let models = registration["models"]
                .as_array()
                .filter(|xs| (1..=128).contains(&xs.len()))
                .ok_or(INVALID)?;
            let mut seen_models = BTreeSet::new();
            for model in models {
                let model = model
                    .as_str()
                    .filter(|model| valid_identifier(model))
                    .ok_or(INVALID)?;
                if !seen_models.insert(model) {
                    return Err(INVALID);
                }
                by_model
                    .entry(model.to_owned())
                    .or_default()
                    .push(plugin_id.to_owned());
            }
        }
        for adapters in by_model.values_mut() {
            adapters.sort_unstable();
        }
        Ok(Self(by_model))
    }

    pub(crate) fn match_device(&self, id: &str, name: &str, model: &str) -> Option<MatchedDevice> {
        let adapters = self.0.get(model)?;
        if !valid_identifier(id) {
            return None;
        }
        Some(MatchedDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            model: model.to_owned(),
            adapters: adapters.clone(),
        })
    }
}

fn valid_identifier(value: &str) -> bool {
    value.len() <= 64
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MatchedDevice {
    pub(crate) id: String,
    name: String,
    model: String,
    adapters: Vec<String>,
}

impl MatchedDevice {
    pub(crate) fn to_json(&self) -> Value {
        json!({"id": self.id, "name": self.name, "model": self.model, "adapters": self.adapters})
    }
}

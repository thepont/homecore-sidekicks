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

#[derive(Clone, Default)]
pub(crate) struct AdapterCatalog(BTreeMap<String, Vec<String>>);

pub(crate) struct AdapterRegistration {
    pub(crate) plugin_id: String,
    models: BTreeSet<String>,
}

impl AdapterRegistration {
    pub(crate) fn parse(value: &Value) -> Result<Self, &'static str> {
        let plugin_id = value["plugin_id"]
            .as_str()
            .filter(|id| valid_plugin_id(id))
            .ok_or(INVALID)?;
        let models = value["models"]
            .as_array()
            .filter(|models| (1..=128).contains(&models.len()))
            .ok_or(INVALID)?;
        let mut registered_models = BTreeSet::new();
        for model in models {
            let model = model
                .as_str()
                .filter(|model| valid_identifier(model))
                .ok_or(INVALID)?;
            if !registered_models.insert(model.to_owned()) {
                return Err(INVALID);
            }
        }
        Ok(Self {
            plugin_id: plugin_id.to_owned(),
            models: registered_models,
        })
    }
}

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

    pub(crate) fn parse(value: &Value) -> Result<Self, &'static str> {
        let registrations = value
            .as_array()
            .filter(|xs| xs.len() <= 32)
            .ok_or(INVALID)?;
        let mut plugin_ids = BTreeSet::new();
        let mut catalog = Self::default();
        for registration in registrations {
            let registration = AdapterRegistration::parse(registration)?;
            if !plugin_ids.insert(registration.plugin_id.clone()) {
                return Err(INVALID);
            }
            catalog.upsert(registration)?;
        }
        Ok(catalog)
    }

    pub(crate) fn upsert(&mut self, registration: AdapterRegistration) -> Result<(), &'static str> {
        let distinct_plugins: BTreeSet<_> = self.0.values().flatten().collect();
        if distinct_plugins.len() == 32 && !distinct_plugins.contains(&registration.plugin_id) {
            return Err("Too many Xiaomi adapter registrations");
        }
        self.0.retain(|_, adapters| {
            adapters.retain(|id| id != &registration.plugin_id);
            !adapters.is_empty()
        });
        for model in registration.models {
            let adapters = self.0.entry(model).or_default();
            adapters.push(registration.plugin_id.clone());
            adapters.sort_unstable();
        }
        Ok(())
    }

    pub(crate) fn supports(&self, plugin_id: &str, model: &str) -> bool {
        self.0
            .get(model)
            .is_some_and(|plugins| plugins.iter().any(|id| id == plugin_id))
    }

    pub(crate) fn adapters(&self, model: &str) -> &[String] {
        self.0.get(model).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn registrations(&self) -> Value {
        let mut by_plugin: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (model, plugins) in &self.0 {
            for plugin in plugins {
                by_plugin.entry(plugin).or_default().push(model);
            }
        }
        Value::Array(
            by_plugin
                .into_iter()
                .map(|(plugin_id, models)| json!({"plugin_id": plugin_id, "models": models}))
                .collect(),
        )
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

pub(crate) fn valid_plugin_id(id: &str) -> bool {
    id.starts_with("plugin.") && valid_identifier(id)
}

pub(crate) fn valid_identifier(value: &str) -> bool {
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
    pub(crate) name: String,
    pub(crate) model: String,
    adapters: Vec<String>,
}

impl MatchedDevice {
    pub(crate) fn to_json(&self) -> Value {
        json!({"id": self.id, "name": self.name, "model": self.model, "adapters": self.adapters})
    }
}

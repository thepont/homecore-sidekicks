//! Validated configuration for one or more explicitly named controllers.
use serde_json::Value;
use std::collections::HashSet;
use tokio_tungstenite::tungstenite::http::Uri;

pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
#[derive(Clone, Debug)]
pub struct Unit {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub units: Vec<Unit>,
    pub publish_interval_secs: u64,
    pub stale_after_secs: u64,
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err("Configuration exceeds size limit");
        }
        let raw: Value = serde_json::from_slice(bytes).map_err(|_| "Invalid configuration JSON")?;
        let object = raw.as_object().ok_or("Configuration must be an object")?;
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "units" | "publish_interval_secs" | "stale_after_secs"
            )
        }) {
            return Err("Unknown configuration field");
        }
        let rows = raw
            .get("units")
            .and_then(Value::as_array)
            .ok_or("Missing units")?;
        if !(1..=32).contains(&rows.len()) {
            return Err("Configure between one and 32 units");
        }
        let mut ids = HashSet::new();
        let mut urls = HashSet::new();
        let mut units = Vec::new();
        for row in rows {
            let object = row.as_object().ok_or("Invalid unit")?;
            if object
                .keys()
                .any(|key| !matches!(key.as_str(), "id" | "name" | "url"))
            {
                return Err("Unknown unit field");
            }
            let id = text(row, "id", 40)?;
            if !id.as_bytes()[0].is_ascii_lowercase()
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                || !ids.insert(id.clone())
            {
                return Err("Unit IDs must be unique lowercase identifiers");
            }
            let name = text(row, "name", 128)?;
            let url = text(row, "url", 255)?;
            let uri: Uri = url.parse().map_err(|_| "Invalid unit URL")?;
            if uri.scheme_str() != Some("ws")
                || uri.path() != "/ws"
                || uri.query().is_some()
                || url.contains(['@', '#'])
                || uri.port_u16() == Some(0)
            {
                return Err("Unit URL must be ws://host[:port]/ws without credentials or query");
            }
            let host = uri.host().ok_or("Missing controller host")?;
            let authority = uri.authority().ok_or("Missing controller host")?;
            if authority.as_str() != host && uri.port_u16().is_none() {
                return Err("Invalid controller port");
            }
            let endpoint = format!(
                "{}:{}",
                host.to_ascii_lowercase(),
                uri.port_u16().unwrap_or(80)
            );
            if !valid_host(host) || !urls.insert(endpoint) {
                return Err("Controller URLs must be valid and unique");
            }
            units.push(Unit { id, name, url });
        }
        Ok(Self {
            units,
            publish_interval_secs: seconds(&raw, "publish_interval_secs", 5, 1, 60)?,
            stale_after_secs: seconds(&raw, "stale_after_secs", 10, 2, 300)?,
        })
    }
}

fn valid_host(host: &str) -> bool {
    if let Some(address) = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
    {
        return address.parse::<std::net::Ipv6Addr>().is_ok();
    }
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.as_bytes()[0].is_ascii_alphanumeric()
            && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn text(value: &Value, key: &str, max: usize) -> Result<String, &'static str> {
    let text = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or("Missing unit field")?;
    if text.trim().is_empty() || text.len() > max || text.chars().any(char::is_control) {
        return Err("Invalid unit text");
    }
    Ok(text.to_owned())
}

fn seconds(
    value: &Value,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, &'static str> {
    let seconds = match value.get(key) {
        Some(value) => value.as_u64().ok_or("Timing must be whole seconds")?,
        None => default,
    };
    (min..=max)
        .contains(&seconds)
        .then_some(seconds)
        .ok_or("Timing is outside its supported range")
}

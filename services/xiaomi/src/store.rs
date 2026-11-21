use ring::rand::{SecureRandom, SystemRandom};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub struct Store {
    root: PathBuf,
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub(crate) fn valid_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Store {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
    pub(crate) fn adapters(&self) -> Result<Option<crate::adapters::AdapterCatalog>, &'static str> {
        crate::adapters::AdapterCatalog::load(&self.root.join("adapters.json"))
    }
    pub fn save(&self, token: &str) -> Result<(), &'static str> {
        if !valid_token(token) {
            return Err("Invalid gateway token");
        }
        if !fs::symlink_metadata(&self.root)
            .map_err(|_| "Secret directory unavailable")?
            .is_dir()
        {
            return Err("Secret directory unavailable");
        }
        let final_path = self.root.join("gateway-token");
        if fs::symlink_metadata(&final_path).is_ok_and(|m| !m.is_file()) {
            return Err("Invalid secret path");
        }
        let mut random = [0u8; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| "Cannot create secret")?;
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let tmp = Temp(self.root.join(format!(".gateway-{suffix}")));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp.0)
            .map_err(|_| "Cannot create secret")?;
        file.write_all(token.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot write secret")?;
        fs::rename(&tmp.0, final_path).map_err(|_| "Cannot save secret")?;
        File::open(&self.root)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Cannot sync secret")?;
        Ok(())
    }
    pub fn load(&self) -> Result<Option<String>, &'static str> {
        let path = self.root.join("gateway-token");
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Cannot read secret"),
        };
        let metadata = file.metadata().map_err(|_| "Cannot read secret")?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("Invalid secret permissions");
        }
        let mut token = String::new();
        file.take(33)
            .read_to_string(&mut token)
            .map_err(|_| "Cannot read secret")?;
        if !valid_token(&token) {
            return Err("Invalid gateway token");
        }
        Ok(Some(token))
    }
}

pub struct GatewayConfig {
    address: String,
    model: String,
    token: String,
    device_id: Option<String>,
    alternate_token: Option<String>,
    cloud_comparison: Option<String>,
}
impl GatewayConfig {
    pub fn new(address: &str, model: &str, token: &str) -> Result<Self, &'static str> {
        let ip: std::net::Ipv4Addr = address.parse().map_err(|_| "Invalid gateway address")?;
        if !ip.is_private()
            || !model.starts_with("lumi.gateway.")
            || model.len() > 128
            || !model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
            || !valid_token(token)
        {
            return Err("Invalid gateway configuration");
        }
        Ok(Self {
            address: address.into(),
            model: model.into(),
            token: token.into(),
            device_id: None,
            alternate_token: None,
            cloud_comparison: None,
        })
    }
    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn with_device_id(mut self, id: &str) -> Result<Self, &'static str> {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("Invalid gateway identity");
        }
        self.device_id = Some(id.to_owned());
        Ok(self)
    }
    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }
    pub fn with_alternate_token(mut self, token: &str) -> Result<Self, &'static str> {
        if !valid_token(token) {
            return Err("Invalid gateway token");
        }
        self.alternate_token = Some(token.to_owned());
        Ok(self)
    }
    pub fn with_cloud_comparison(mut self, status: &str) -> Result<Self, &'static str> {
        if !matches!(status, "same" | "different" | "not_found" | "unavailable") {
            return Err("Invalid discovery status");
        }
        self.cloud_comparison = Some(status.to_owned());
        Ok(self)
    }
    pub fn cloud_comparison(&self) -> Option<&str> {
        self.cloud_comparison.as_deref()
    }
    pub fn alternate_token(&self) -> Option<&str> {
        self.alternate_token.as_deref()
    }
    pub fn token(&self) -> &str {
        &self.token
    }
}
impl Store {
    pub fn save_gateway(&self, config: &GatewayConfig) -> Result<(), &'static str> {
        if !fs::symlink_metadata(&self.root)
            .map_err(|_| "Secret directory unavailable")?
            .is_dir()
        {
            return Err("Secret directory unavailable");
        }
        let destination = self.root.join("gateway.json");
        if fs::symlink_metadata(&destination).is_ok_and(|m| !m.is_file()) {
            return Err("Invalid secret path");
        }
        let mut random = [0u8; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| "Cannot create secret")?;
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let tmp = Temp(self.root.join(format!(".gateway-config-{suffix}")));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp.0)
            .map_err(|_| "Cannot create secret")?;
        let data = serde_json::json!({"address":config.address, "model":config.model, "token":config.token, "device_id":config.device_id, "alternate_token":config.alternate_token, "cloud_comparison":config.cloud_comparison});
        serde_json::to_writer(&mut file, &data).map_err(|_| "Cannot write secret")?;
        file.sync_all().map_err(|_| "Cannot write secret")?;
        fs::rename(&tmp.0, destination).map_err(|_| "Cannot save secret")?;
        File::open(&self.root)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Cannot sync secret")?;
        Ok(())
    }
    pub fn load_gateway(&self) -> Result<Option<GatewayConfig>, &'static str> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.root.join("gateway.json"))
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Cannot read secret"),
        };
        let metadata = file.metadata().map_err(|_| "Cannot read secret")?;
        if !metadata.is_file()
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.len() > 1024
        {
            return Err("Invalid secret permissions or size");
        }
        let value: serde_json::Value = serde_json::from_reader(file.take(1025))
            .map_err(|_| "Invalid gateway configuration")?;
        let field = |name| {
            value
                .get(name)
                .and_then(|v| v.as_str())
                .ok_or("Invalid gateway configuration")
        };
        let mut config = GatewayConfig::new(field("address")?, field("model")?, field("token")?)?;
        match value.get("device_id") {
            None | Some(serde_json::Value::Null) => {}
            Some(serde_json::Value::String(id)) => config = config.with_device_id(id)?,
            Some(_) => return Err("Invalid gateway identity"),
        }
        match value.get("alternate_token") {
            None | Some(serde_json::Value::Null) => {}
            Some(serde_json::Value::String(token)) => {
                config = config.with_alternate_token(token)?
            }
            Some(_) => return Err("Invalid gateway token"),
        }
        match value.get("cloud_comparison") {
            None | Some(serde_json::Value::Null) => {}
            Some(serde_json::Value::String(status)) => {
                config = config.with_cloud_comparison(status)?
            }
            Some(_) => return Err("Invalid discovery status"),
        }
        Ok(Some(config))
    }
}

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
fn valid(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Store {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
    pub fn save(&self, token: &str) -> Result<(), &'static str> {
        if !valid(token) {
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
        if !valid(&token) {
            return Err("Invalid gateway token");
        }
        Ok(Some(token))
    }
}

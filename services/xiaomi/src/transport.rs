use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, path::Path, time::Duration};
use zeroize::Zeroizing;

pub(crate) const MAX_SESSION: Duration = Duration::from_secs(600);
pub(crate) const MAX_MODELS: usize = 1024;

pub(crate) fn valid_identifier(value: &str) -> bool {
    value.len() <= 64
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

pub(crate) fn valid_region(region: &str) -> bool {
    matches!(region, "cn" | "de" | "i2" | "ru" | "sg" | "us")
}

pub(crate) fn bearer(key: &[u8; 32]) -> Zeroizing<String> {
    use std::fmt::Write;
    let mut secret = Zeroizing::new(String::with_capacity(64));
    for byte in key {
        write!(secret, "{byte:02x}").expect("formatting into a String cannot fail");
    }
    secret
}

pub fn read_service_key(path: &Path) -> Result<[u8; 32], &'static str> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Cannot read extractor key")?;
    let metadata = file.metadata().map_err(|_| "Cannot read extractor key")?;
    if !metadata.is_file() || metadata.len() != 32 {
        return Err("Extractor key must be 32 bytes");
    }
    let mut key = Zeroizing::new([0; 32]);
    file.read_exact(&mut *key)
        .map_err(|_| "Cannot read extractor key")?;
    if file
        .read(&mut [0])
        .map_err(|_| "Cannot read extractor key")?
        != 0
    {
        return Err("Extractor key must be 32 bytes");
    }
    Ok(*key)
}

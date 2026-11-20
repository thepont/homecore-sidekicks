//! The small homeCore UI management contract for disk-backed plugin bundles.
//!
//! Protocol reference: homeCore commit 40a9c649e33bd61d8cbb343eebf20b61bd624ca3,
//! sdk/rust/src/{ui,lib}.rs. Assets remain on disk: startup retains only the
//! validated descriptor, file lengths and SHA-256 hashes. Each request verifies
//! its file with an 8 KiB scratch buffer while collecting only the requested
//! chunk. The caller serialises requests and bounds the MQTT response queue.
//! Files are deployed immutably; changed bytes are refused until restart gives
//! the bundle a new build address. No file handle or asset-body cache is kept.

use base64::Engine;
use ring::digest::{Context, Digest, SHA256};
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_REQUEST_BYTES: usize = 4096;
pub const MAX_CHUNK_BYTES: usize = 128 * 1024;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024;
const MAX_ASSET_BYTES: u64 = 512 * 1024;

#[derive(Debug)]
pub struct UiError;

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UI assets are invalid or unavailable")
    }
}
impl std::error::Error for UiError {}

#[derive(Debug)]
pub struct UiAssets {
    dir: PathBuf,
    descriptor: Value,
    files: [Asset; 2],
}

#[derive(Debug)]
struct Asset {
    name: &'static str,
    size: u64,
    digest: [u8; 32],
}

impl From<std::io::Error> for UiError {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

impl From<serde_json::Error> for UiError {
    fn from(_: serde_json::Error) -> Self {
        Self
    }
}

impl UiAssets {
    /// Read and validate the two-file bundle without retaining either body.
    pub fn load(
        dir: &Path,
        version: &str,
        validate_slots: impl FnOnce(&[Value]) -> Result<(), UiError>,
    ) -> Result<Self, UiError> {
        if version.is_empty()
            || version.len() > 64
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b))
        {
            return Err(UiError);
        }
        if !fs::symlink_metadata(dir)?.is_dir() {
            return Err(UiError);
        }
        let dir = dir.canonicalize()?;
        let mut raw = Vec::new();
        open_asset(&dir, "manifest.json")?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut raw)?;
        if raw.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(UiError);
        }
        let manifest: Value = serde_json::from_slice(&raw)?;
        let object = manifest.as_object().ok_or(UiError)?;
        if object
            .keys()
            .any(|k| !["host_api", "entry", "slots"].contains(&k.as_str()))
            || manifest["host_api"] != 1
            || manifest["entry"] != "index.js"
        {
            return Err(UiError);
        }
        validate_slots(manifest["slots"].as_array().ok_or(UiError)?)?;
        let manifest_hash = ring::digest::digest(&SHA256, &raw);
        let mut build = Context::new(&SHA256);
        let mut scan = |name: &'static str| -> Result<Asset, UiError> {
            let mut f = open_asset(&dir, name)?;
            let size = f.metadata()?.len();
            build.update(name.as_bytes());
            build.update(&[0]);
            let hash = scan_asset(&mut f, size, |_, chunk| build.update(chunk))?;
            Ok(Asset {
                name,
                size,
                digest: hash.as_ref().try_into().unwrap(),
            })
        };
        let files = [scan("index.js")?, scan("manifest.json")?];
        if files[1].digest != manifest_hash.as_ref() {
            return Err(UiError);
        }
        let build = build.finish().as_ref()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(Self {
            dir,
            files,
            descriptor: json!({
                "host_api": manifest["host_api"], "entry": manifest["entry"],
                "slots": manifest["slots"], "version": version, "build": build
            }),
        })
    }
    pub fn descriptor(&self) -> Value {
        self.descriptor.clone()
    }
    pub fn respond(&self, payload: &[u8]) -> Option<Value> {
        reply(Some(self), payload)
    }

    fn piece(&self, index: usize, offset: u64, max: usize) -> Result<(Vec<u8>, bool), UiError> {
        let asset = &self.files[index];
        if offset > asset.size {
            return Err(UiError);
        }
        let mut file = open_asset(&self.dir, asset.name)?;
        if file.metadata()?.len() != asset.size {
            return Err(UiError);
        }
        let end = (offset + max as u64).min(asset.size);
        let mut bytes = Vec::with_capacity((end - offset) as usize);
        let hash = scan_asset(&mut file, asset.size, |position, chunk| {
            let start = position.max(offset);
            let stop = (position + chunk.len() as u64).min(end);
            if stop > start {
                bytes.extend_from_slice(
                    &chunk[(start - position) as usize..(stop - position) as usize],
                );
            }
        })?;
        if hash.as_ref() != asset.digest {
            return Err(UiError);
        }
        Ok((bytes, end == asset.size))
    }
}

fn scan_asset(
    file: &mut File,
    size: u64,
    mut visit: impl FnMut(u64, &[u8]),
) -> Result<Digest, UiError> {
    let mut hash = Context::new(&SHA256);
    let mut buffer = [0u8; 8192];
    let mut position = 0;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let next = position + n as u64;
        if next > size {
            return Err(UiError);
        }
        let chunk = &buffer[..n];
        hash.update(chunk);
        visit(position, chunk);
        position = next;
    }
    if position != size {
        return Err(UiError);
    }
    Ok(hash.finish())
}

fn open_asset(dir: &Path, name: &str) -> Result<File, UiError> {
    if !fs::symlink_metadata(dir)?.is_dir() {
        return Err(UiError);
    }
    let path = dir.join(name);
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err(UiError);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    let limit = if name == "manifest.json" {
        MAX_MANIFEST_BYTES
    } else {
        MAX_ASSET_BYTES
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
        return Err(UiError);
    }
    Ok(file)
}

fn failure(id: &str, error: &str) -> Value {
    json!({"request_id":id,"status":"error","error":error})
}

pub fn respond_unavailable(payload: &[u8]) -> Option<Value> {
    reply(None, payload)
}

enum Operation {
    Ping,
    File {
        index: usize,
        offset: u64,
        max: usize,
    },
    Denied(&'static str),
}

struct Request {
    id: String,
    operation: Operation,
}

fn parse_request(payload: &[u8]) -> Option<Request> {
    if payload.len() > MAX_REQUEST_BYTES {
        return None;
    }
    let value: Value = serde_json::from_slice(payload).ok()?;
    let obj = value.as_object()?;
    let id = obj.get("request_id")?.as_str()?;
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    let operation = match obj.get("action").and_then(Value::as_str) {
        Some("ping")
            if obj
                .keys()
                .all(|k| ["action", "request_id"].contains(&k.as_str())) =>
        {
            Operation::Ping
        }
        Some("ui_file") => {
            let number = |field, default| match obj.get(field) {
                None => Some(default),
                Some(v) => v.as_u64(),
            };
            let index = match obj.get("file").and_then(Value::as_str) {
                Some("index.js") => Some(0),
                Some("manifest.json") => Some(1),
                _ => None,
            };
            match (
                index,
                number("offset", 0),
                number("max", MAX_CHUNK_BYTES as u64),
            ) {
                (Some(index), Some(offset), Some(max))
                    if offset <= MAX_ASSET_BYTES
                        && (1..=MAX_CHUNK_BYTES as u64).contains(&max)
                        && obj.keys().all(|k| {
                            ["action", "request_id", "file", "offset", "max"].contains(&k.as_str())
                        }) =>
                {
                    Operation::File {
                        index,
                        offset,
                        max: max as usize,
                    }
                }
                _ => Operation::Denied("no such file"),
            }
        }
        _ => Operation::Denied("Unsupported management action"),
    };
    Some(Request {
        id: id.to_owned(),
        operation,
    })
}

fn reply(assets: Option<&UiAssets>, payload: &[u8]) -> Option<Value> {
    let Request { id, operation } = parse_request(payload)?;
    Some(match operation {
        Operation::Ping => json!({"request_id":id,"status":"ok"}),
        Operation::Denied(error) => failure(&id, error),
        Operation::File { index, offset, max } => match assets
            .ok_or(UiError)
            .and_then(|a| a.piece(index, offset, max))
        {
            Ok((bytes, done)) => {
                json!({"request_id":id,"status":"ok","data":base64::engine::general_purpose::STANDARD.encode(bytes),"done":done})
            }
            Err(_) => failure(&id, "no such file"),
        },
    })
}

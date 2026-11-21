use crate::{
    adapters::AdapterCatalog,
    login::{DiscoveredDevice, LanDevice, MAX_DISCOVERED_DEVICES},
    transport::{bearer, read_service_key, MAX_MODELS, MAX_SESSION},
};
use reqwest::{Client, Response, Url};
use rustls::{
    pki_types::{pem::PemObject, CertificateDer},
    RootCertStore,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path, time::Duration};
use zeroize::{Zeroize, Zeroizing};

const ERROR: &str = "Xiaomi extractor unavailable";
const MAX_FRAME: usize = 270 * 1024;

pub(crate) struct ExtractorClient {
    url: Url,
    client: Client,
    auth: Zeroizing<String>,
}

pub(crate) struct Extraction {
    pub(crate) qr_png: String,
    responses: Frames,
}

impl ExtractorClient {
    pub(crate) fn new(
        origin: &str,
        key_file: &Path,
        ca_file: Option<&Path>,
    ) -> Result<Self, &'static str> {
        let url = Url::parse(origin).map_err(|_| "Invalid Xiaomi extractor URL")?;
        let local_fixture = url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
        if !(local_fixture || url.scheme() == "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Invalid Xiaomi extractor URL");
        }
        let mut key = read_service_key(key_file)?;
        let auth = bearer(&key);
        key.zeroize();
        let mut roots = RootCertStore::empty();
        if let Some(path) = ca_file {
            for certificate in read_ca(path)? {
                roots.add(certificate).map_err(|_| ERROR)?;
            }
        }
        let client = Client::builder()
            .tls_backend_preconfigured(homecore_bounded_http::tls::client_config(Some(roots)))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(MAX_SESSION + Duration::from_secs(30));
        Ok(Self {
            url: url.join("/v1/discover").map_err(|_| ERROR)?,
            client: client.build().map_err(|_| ERROR)?,
            auth,
        })
    }

    pub(crate) async fn begin(
        &self,
        region: &str,
        catalog: &AdapterCatalog,
    ) -> Result<Extraction, &'static str> {
        let models = catalog.models();
        if models.len() > MAX_MODELS {
            return Err("Too many Xiaomi adapter models");
        }
        tokio::time::timeout(Duration::from_secs(22), async {
            let response = self
                .client
                .post(self.url.clone())
                .bearer_auth(self.auth.as_str())
                .json(&json!({"region":region,"models":models}))
                .send()
                .await
                .map_err(|_| ERROR)?
                .error_for_status()
                .map_err(|_| ERROR)?;
            let mut responses = Frames {
                response,
                buffer: Vec::new(),
            };
            let first = responses.next().await?;
            let qr_png = first["qr_png"]
                .as_str()
                .filter(|qr| {
                    qr.len() <= 262144
                        && qr.starts_with("iVBORw0KGgo")
                        && qr.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
                        })
                })
                .ok_or(ERROR)?;
            if first["type"] != "qr" {
                return Err(ERROR);
            }
            Ok(Extraction {
                qr_png: qr_png.to_owned(),
                responses,
            })
        })
        .await
        .map_err(|_| ERROR)?
    }
}

impl Extraction {
    pub(crate) async fn finish(
        mut self,
        catalog: &AdapterCatalog,
        capture: bool,
    ) -> Result<Vec<DiscoveredDevice>, &'static str> {
        let frame = self.responses.next().await?;
        if frame["type"] != "devices" {
            return Err(ERROR);
        }
        let devices = frame["devices"]
            .as_array()
            .filter(|rows| rows.len() <= MAX_DISCOVERED_DEVICES)
            .ok_or(ERROR)?;
        let mut ids = BTreeSet::new();
        let mut result = Vec::with_capacity(devices.len());
        for item in devices {
            let lan = LanDevice::from_cloud(item).ok_or(ERROR)?;
            let id = lan.device_id.ok_or(ERROR)?;
            let device = lan.match_adapter(catalog, capture).ok_or(ERROR)?;
            if !ids.insert(id) {
                return Err(ERROR);
            }
            result.push(device);
        }
        Ok(result)
    }
}

struct Frames {
    response: Response,
    buffer: Vec<u8>,
}

impl Frames {
    async fn next(&mut self) -> Result<Value, &'static str> {
        loop {
            if let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
                if end > MAX_FRAME {
                    return Err(ERROR);
                }
                let payload = Zeroizing::new(self.buffer.drain(..=end).collect::<Vec<_>>());
                return serde_json::from_slice(&payload).map_err(|_| ERROR);
            }
            let chunk = self
                .response
                .chunk()
                .await
                .map_err(|_| ERROR)?
                .ok_or(ERROR)?;
            if self.buffer.len().saturating_add(chunk.len()) > 2 * MAX_FRAME {
                return Err(ERROR);
            }
            self.buffer.extend_from_slice(&chunk);
        }
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        self.buffer.zeroize();
    }
}

fn read_ca(path: &Path) -> Result<Vec<CertificateDer<'static>>, &'static str> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| ERROR)?;
    if !file.metadata().map_err(|_| ERROR)?.is_file() {
        return Err(ERROR);
    }
    let mut pem = Vec::new();
    file.take(65537).read_to_end(&mut pem).map_err(|_| ERROR)?;
    if pem.len() > 65536 {
        return Err(ERROR);
    }
    let certs: Vec<_> = CertificateDer::pem_slice_iter(&pem)
        .collect::<Result<_, _>>()
        .map_err(|_| ERROR)?;
    if certs.is_empty() {
        return Err(ERROR);
    }
    Ok(certs)
}

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Url};
use serde_json::Value;
use std::{
    net::Ipv4Addr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Provider {
    base: Url,
    client: Client,
}
pub struct Challenge {
    pub qr_png: String,
    poll_url: Url,
    pub expires_after: Duration,
}
impl Provider {
    pub fn new(base: &str) -> Result<Self, &'static str> {
        let base = Url::parse(base).map_err(|_| "Invalid provider")?;
        let test = base.scheme() == "http"
            && base
                .host_str()
                .is_some_and(|h| h == "127.0.0.1" || h == "::1");
        if !(test || base.scheme() == "https" && base.host_str() == Some("account.xiaomi.com"))
            || !base.username().is_empty()
            || base.password().is_some()
            || base.path() != "/"
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err("Invalid provider");
        }
        let client = Client::builder()
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "Cannot create provider client")?;
        Ok(Self { base, client })
    }
    fn allowed(&self, raw: &str) -> Result<Url, &'static str> {
        let url = Url::parse(raw).map_err(|_| "Invalid provider response")?;
        let same_test = self.base.scheme() == "http" && url.origin() == self.base.origin();
        let xiaomi = url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|h| h == "account.xiaomi.com" || h.ends_with(".account.xiaomi.com"));
        if !(same_test || xiaomi)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("Invalid provider response");
        }
        Ok(url)
    }
    pub async fn begin(&self) -> Result<Challenge, &'static str> {
        let mut url = self
            .base
            .join("/longPolling/loginUrl")
            .map_err(|_| "Invalid provider")?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Invalid clock")?
            .as_millis()
            .to_string();
        url.query_pairs_mut().extend_pairs([
            ("_qrsize", "480"),
            ("qs", "%3Fsid%3Dxiaomiio%26_json%3Dtrue"),
            ("callback", "https://sts.api.io.mi.com/sts"),
            ("_hasLogo", "false"),
            ("sid", "xiaomiio"),
            ("serviceParam", ""),
            ("_locale", "en_GB"),
            ("_dc", timestamp.as_str()),
        ]);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "Xiaomi sign-in unavailable")?
            .error_for_status()
            .map_err(|_| "Xiaomi sign-in unavailable")?;
        let data = bounded(response, 65536).await?;
        let data = data.strip_prefix(b"&&&START&&&").unwrap_or(&data);
        let body: Value = serde_json::from_slice(data).map_err(|_| "Invalid provider response")?;
        let qr = self.allowed(
            body.get("qr")
                .and_then(Value::as_str)
                .ok_or("Invalid provider response")?,
        )?;
        let poll_url = self.allowed(
            body.get("lp")
                .and_then(Value::as_str)
                .ok_or("Invalid provider response")?,
        )?;
        let seconds = body
            .get("timeout")
            .and_then(Value::as_u64)
            .filter(|v| (1..=600).contains(v))
            .ok_or("Invalid provider response")?;
        let image = self
            .client
            .get(qr)
            .send()
            .await
            .map_err(|_| "Xiaomi sign-in unavailable")?
            .error_for_status()
            .map_err(|_| "Xiaomi sign-in unavailable")?;
        let bytes = bounded(image, 192 * 1024).await?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("Invalid QR image");
        }
        Ok(Challenge {
            qr_png: STANDARD.encode(bytes),
            poll_url,
            expires_after: Duration::from_secs(seconds),
        })
    }
}
async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Xiaomi response failed")?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("Xiaomi response too large");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Cloud credentials remain inside the service; they are never returned to the setup page.
pub struct Authentication {
    user_id: String,
    security: String,
    service_token: String,
}
impl Authentication {
    pub fn is_authenticated(&self) -> bool {
        !self.user_id.is_empty() && !self.security.is_empty() && !self.service_token.is_empty()
    }
}
impl Provider {
    pub async fn poll(&self, challenge: &Challenge) -> Result<Authentication, &'static str> {
        let response = self
            .client
            .get(challenge.poll_url.clone())
            .timeout(challenge.expires_after)
            .send()
            .await
            .map_err(|_| "Waiting for Xiaomi sign-in failed")?
            .error_for_status()
            .map_err(|_| "Waiting for Xiaomi sign-in failed")?;
        let data = bounded(response, 65536).await?;
        let data = data.strip_prefix(b"&&&START&&&").unwrap_or(&data);
        let body: Value = serde_json::from_slice(data).map_err(|_| "Invalid provider response")?;
        let user_id = match body.get("userId") {
            Some(Value::String(s))
                if !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_digit()) =>
            {
                s.clone()
            }
            Some(Value::Number(n)) if n.is_u64() => n.to_string(),
            _ => return Err("Invalid provider response"),
        };
        let security = body
            .get("ssecurity")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 4096 && STANDARD.decode(s).is_ok())
            .ok_or("Invalid provider response")?
            .to_owned();
        let raw = body
            .get("location")
            .and_then(Value::as_str)
            .ok_or("Invalid provider response")?;
        let url = Url::parse(raw).map_err(|_| "Invalid provider response")?;
        let test = self.base.scheme() == "http" && self.base.origin() == url.origin();
        let trusted = url.scheme() == "https" && url.host_str() == Some("sts.api.io.mi.com");
        if !(test || trusted)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("Invalid provider response");
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "Xiaomi sign-in failed")?
            .error_for_status()
            .map_err(|_| "Xiaomi sign-in failed")?;
        let service_token = response
            .cookies()
            .find(|c| c.name() == "serviceToken")
            .map(|c| c.value().to_owned())
            .filter(|s| !s.is_empty() && s.len() <= 4096 && s.bytes().all(|b| b.is_ascii_graphic()))
            .ok_or("Invalid provider response")?;
        Ok(Authentication {
            user_id,
            security,
            service_token,
        })
    }
}

#[cfg(feature = "setup")]
pub struct Gateway {
    pub name: String,
    pub model: String,
    pub address: String,
    device_id: Option<String>,
    token: String,
    alternate_token: Option<String>,
    cloud_comparison: &'static str,
}

pub(crate) struct LanDevice<'a> {
    pub(crate) name: &'a str,
    pub(crate) model: &'a str,
    pub(crate) address: Ipv4Addr,
    pub(crate) device_id: Option<&'a str>,
    pub(crate) token: &'a str,
}

#[derive(PartialEq, Eq)]
#[cfg(feature = "setup")]
pub(crate) struct DiscoveredDevice {
    pub(crate) device: crate::adapters::MatchedDevice,
    pub(crate) credential: Option<DiscoveredCredential>,
}

#[derive(PartialEq, Eq)]
#[cfg(feature = "setup")]
pub(crate) struct DiscoveredCredential {
    pub(crate) address: Ipv4Addr,
    pub(crate) token: String,
}

#[cfg(feature = "setup")]
impl Drop for DiscoveredCredential {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
    }
}

impl<'a> LanDevice<'a> {
    pub(crate) fn from_cloud(item: &'a Value) -> Option<Self> {
        Some(Self {
            name: item.get("name").and_then(Value::as_str).filter(|name| {
                !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
            })?,
            model: item.get("model").and_then(Value::as_str)?,
            address: item
                .get("localip")
                .and_then(Value::as_str)
                .and_then(|address| address.parse::<Ipv4Addr>().ok())
                .filter(Ipv4Addr::is_private)?,
            device_id: item.get("did").and_then(Value::as_str),
            token: cloud_device_token(item)?,
        })
    }

    #[cfg(feature = "setup")]
    pub(crate) fn match_adapter(
        &self,
        catalog: &crate::adapters::AdapterCatalog,
        capture_credentials: bool,
    ) -> Option<DiscoveredDevice> {
        let device = catalog.match_device(self.device_id?, self.name, self.model)?;
        let credential = capture_credentials.then(|| DiscoveredCredential {
            address: self.address,
            token: self.token.to_owned(),
        });
        Some(DiscoveredDevice { device, credential })
    }
}

fn cloud_device_token(item: &Value) -> Option<&str> {
    item.get("token")
        .and_then(Value::as_str)
        .filter(|token| valid_token(token))
}

pub(crate) fn valid_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(feature = "setup")]
impl Gateway {
    pub fn save_configuration(&self, store: &crate::store::Store) -> Result<(), &'static str> {
        let mut config = crate::store::GatewayConfig::new(&self.address, &self.model, &self.token)?;
        if let Some(id) = &self.device_id {
            config = config.with_device_id(id)?;
        }
        if let Some(token) = &self.alternate_token {
            config = config.with_alternate_token(token)?;
        }
        config = config.with_cloud_comparison(self.cloud_comparison)?;
        store.save_gateway(&config)
    }
    pub fn save_token(&self, store: &crate::store::Store) -> Result<(), &'static str> {
        store.save(&self.token)
    }
}

impl Provider {
    async fn cloud_call(
        &self,
        auth: &Authentication,
        region: &str,
        path: &str,
        data: Value,
    ) -> Result<Value, &'static str> {
        use ring::{
            digest::{digest, SHA256},
            rand::{SecureRandom, SystemRandom},
        };
        if !matches!(region, "cn" | "de" | "i2" | "ru" | "sg" | "us")
            || auth
                .service_token
                .bytes()
                .any(|b| !b.is_ascii_graphic() || b == b';' || b == b',' || b == b'\\')
        {
            return Err("Invalid cloud session");
        }
        let origin = if self.base.scheme() == "http" {
            self.base.clone()
        } else {
            let host = if region == "cn" {
                "api.io.mi.com".to_owned()
            } else {
                format!("{region}.api.io.mi.com")
            };
            Url::parse(&format!("https://{host}/")).map_err(|_| "Invalid cloud region")?
        };
        let url = origin
            .join(&format!("/app{path}"))
            .map_err(|_| "Invalid cloud request")?;
        let mut nonce = [0u8; 12];
        SystemRandom::new()
            .fill(&mut nonce[..8])
            .map_err(|_| "Cannot start discovery")?;
        let minutes = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Invalid clock")?
            .as_secs()
            / 60;
        let minutes = u32::try_from(minutes).map_err(|_| "Invalid clock")?;
        nonce[8..].copy_from_slice(&minutes.to_be_bytes());
        let payload = data.to_string();
        let signed = crate::cloud::sign_request(path, &payload, &auth.security, &nonce)?;
        let nonce_encoded = STANDARD.encode(nonce);
        let cookie = format!(
            "userId={}; serviceToken={}; yetAnotherServiceToken={}",
            auth.user_id, auth.service_token, auth.service_token
        );
        let response = self
            .client
            .post(url)
            .header("cookie", cookie)
            .header("accept-encoding", "identity")
            .header("x-xiaomi-protocal-flag-cli", "PROTOCAL-HTTP2")
            .header("miot-encrypt-algorithm", "ENCRYPT-RC4")
            .query(&[
                ("data", signed.data.as_str()),
                ("rc4_hash__", signed.hash.as_str()),
                ("signature", signed.signature.as_str()),
                ("ssecurity", auth.security.as_str()),
                ("_nonce", nonce_encoded.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "Xiaomi discovery unavailable")?
            .error_for_status()
            .map_err(|_| "Xiaomi discovery unavailable")?;
        let encrypted = bounded(response, 1024 * 1024).await?;
        let encrypted = STANDARD
            .decode(encrypted)
            .map_err(|_| "Invalid discovery response")?;
        let mut material = STANDARD
            .decode(&auth.security)
            .map_err(|_| "Invalid cloud session")?;
        material.extend_from_slice(&nonce);
        let digest = digest(&SHA256, &material);
        let key: [u8; 32] = digest
            .as_ref()
            .try_into()
            .map_err(|_| "Invalid cloud session")?;
        let data = crate::cloud::legacy_encrypt(&key, &encrypted);
        serde_json::from_slice(&data).map_err(|_| "Invalid discovery response")
    }
    pub(crate) async fn visit_devices(
        &self,
        auth: &Authentication,
        region: &str,
        mut visit: impl FnMut(LanDevice<'_>) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let homes=self.cloud_call(auth,region,"/v2/homeroom/gethome",serde_json::json!({"fg":true,"fetch_share":true,"fetch_share_dev":true,"limit":300,"app_ver":7})).await?;
        let homes = homes
            .pointer("/result/homelist")
            .and_then(Value::as_array)
            .ok_or("Invalid discovery response")?;
        if homes.len() > 300 {
            return Err("Invalid discovery response");
        }
        let owner = auth
            .user_id
            .parse::<u64>()
            .map_err(|_| "Invalid cloud session")?;
        for home in homes {
            let id = home
                .get("id")
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .ok_or("Invalid discovery response")?;
            let devices=self.cloud_call(auth,region,"/v2/home/home_device_list",serde_json::json!({"home_owner":owner,"home_id":id,"limit":200,"get_split_device":true,"support_smart_home":true})).await?;
            let devices = devices
                .pointer("/result/device_info")
                .and_then(Value::as_array)
                .ok_or("Invalid discovery response")?;
            if devices.len() > 200 {
                return Err("Invalid discovery response");
            }
            for item in devices {
                if let Some(device) = LanDevice::from_cloud(item) {
                    visit(device)?;
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "setup")]
    pub(crate) async fn discover_registered(
        &self,
        auth: &Authentication,
        region: &str,
        catalog: &crate::adapters::AdapterCatalog,
        capture_credentials: bool,
    ) -> Result<Vec<DiscoveredDevice>, &'static str> {
        use std::collections::{btree_map::Entry, BTreeMap};
        let mut candidates = BTreeMap::new();
        self.visit_devices(auth, region, |device| {
            if let Some(candidate) = device.match_adapter(catalog, capture_credentials) {
                match candidates.entry(candidate.device.id.clone()) {
                    Entry::Occupied(entry) if entry.get() != &candidate => {
                        return Err("Conflicting Xiaomi device identity");
                    }
                    Entry::Occupied(_) => {}
                    Entry::Vacant(entry) => {
                        entry.insert(candidate);
                    }
                }
                if candidates.len() > 64 {
                    return Err("Too many Xiaomi devices");
                }
            }
            Ok(())
        })
        .await?;
        Ok(candidates.into_values().collect())
    }

    #[cfg(feature = "setup")]
    pub async fn discover(
        &self,
        auth: &Authentication,
        region: &str,
    ) -> Result<Vec<Gateway>, &'static str> {
        let mut gateways = Vec::new();
        self.visit_devices(auth, region, |device| {
            if !device.model.starts_with("lumi.gateway.") || device.model.len() > 64 {
                return Ok(());
            }
            gateways.push(Gateway {
                name: device.name.to_owned(),
                model: device.model.to_owned(),
                address: device.address.to_string(),
                device_id: device.device_id.map(str::to_owned),
                token: device.token.to_owned(),
                alternate_token: None,
                cloud_comparison: "unavailable",
            });
            if gateways.len() > 64 {
                return Err("Too many gateways");
            }
            Ok(())
        })
        .await?;
        if !gateways.is_empty() {
            let payload = serde_json::json!({"getVirtualModel":true,"getHuamiDevices":1,"get_split_device":false,"support_smart_home":true});
            if let Ok(result) = self
                .cloud_call(auth, region, "/home/device_list", payload)
                .await
            {
                if let Some(devices) = result
                    .pointer("/result/list")
                    .and_then(Value::as_array)
                    .filter(|xs| xs.len() <= 4096)
                {
                    for gateway in &mut gateways {
                        gateway.cloud_comparison = "not_found";
                        if let Some(item) = devices.iter().find(|item| {
                            item.get("did").and_then(Value::as_str) == gateway.device_id.as_deref()
                                && gateway.device_id.is_some()
                        }) {
                            gateway.cloud_comparison = "unavailable";
                            if let Some(token) = cloud_device_token(item) {
                                if !token.eq_ignore_ascii_case(&gateway.token) {
                                    gateway.alternate_token = Some(token.to_owned());
                                    gateway.cloud_comparison = "different";
                                } else {
                                    gateway.cloud_comparison = "same";
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(gateways)
    }
}

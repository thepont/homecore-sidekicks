use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Url};
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        let body = provider_json(&data)?;
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

fn provider_json(data: &[u8]) -> Result<Value, &'static str> {
    let data = data.strip_prefix(b"&&&START&&&").unwrap_or(data);
    serde_json::from_slice(data).map_err(|_| "Invalid provider response")
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
        let body = provider_json(&data)?;
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

pub struct Gateway {
    pub name: String,
    pub model: String,
    pub address: String,
    device_id: Option<String>,
    token: String,
    alternate_token: Option<String>,
    cloud_comparison: &'static str,
}
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
    pub async fn discover(
        &self,
        auth: &Authentication,
        region: &str,
    ) -> Result<Vec<Gateway>, &'static str> {
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
        let mut gateways = Vec::new();
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
                let Some(model) = item.get("model").and_then(Value::as_str) else {
                    continue;
                };
                if !model.starts_with("lumi.gateway.") || model.len() > 64 {
                    continue;
                }
                let Some(address) = item
                    .get("localip")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<std::net::Ipv4Addr>().ok())
                    .filter(|ip| ip.is_private())
                else {
                    continue;
                };
                let Some(token) = item
                    .get("token")
                    .and_then(Value::as_str)
                    .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                else {
                    continue;
                };
                let Some(name) = item.get("name").and_then(Value::as_str).filter(|s| {
                    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
                }) else {
                    continue;
                };
                gateways.push(Gateway {
                    name: name.to_owned(),
                    model: model.to_owned(),
                    address: address.to_string(),
                    device_id: item.get("did").and_then(Value::as_str).map(str::to_owned),
                    token: token.to_owned(),
                    alternate_token: None,
                    cloud_comparison: "unavailable",
                });
                if gateways.len() > 64 {
                    return Err("Too many gateways");
                }
            }
        }
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
                            if let Some(token) =
                                item.get("token").and_then(Value::as_str).filter(|s| {
                                    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
                                })
                            {
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

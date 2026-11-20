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

use serde_json::Value;
use std::io::Read;

pub fn load_kw(status: &Value) -> Result<f64, &'static str> {
    let watts = status["response"]["load_power"]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or("Invalid Teslemetry load reading")?;
    Ok(watts / 1000.0)
}

fn fetch_status(
    client: &reqwest::blocking::Client,
    base: &str,
    site: u64,
    token: &str,
) -> Result<Value, &'static str> {
    let url = reqwest::Url::parse(base).map_err(|_| "Invalid Teslemetry configuration")?;
    if !(url.scheme() == "https" || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || site == 0
        || token.is_empty()
        || token.len() > 4096
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err("Invalid Teslemetry configuration");
    }
    let response = client
        .get(format!(
            "{}/api/1/energy_sites/{site}/live_status",
            base.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|_| "Teslemetry request failed")?;
    let mut body = Vec::new();
    response
        .take(65_537)
        .read_to_end(&mut body)
        .map_err(|_| "Teslemetry request failed")?;
    if body.len() > 65_536 {
        return Err("Invalid Teslemetry response");
    }
    serde_json::from_slice(&body).map_err(|_| "Invalid Teslemetry response")
}

pub fn fetch_load_kw(
    client: &reqwest::blocking::Client,
    base: &str,
    site: u64,
    token: &str,
) -> Result<f64, &'static str> {
    load_kw(&fetch_status(client, base, site, token)?)
}

#[derive(Debug, PartialEq)]
pub struct Powerwall {
    pub charge_pct: f64,
    pub power_w: f64,
}

pub fn powerwall(status: &Value) -> Result<Powerwall, &'static str> {
    let response = &status["response"];
    let observed = response["timestamp"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .ok_or("Invalid Powerwall timestamp")?;
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let age = now.signed_duration_since(observed).num_seconds();
    if !(-120..=300).contains(&age) {
        return Err("Stale Powerwall reading");
    }
    let charge_pct = response["percentage_charged"]
        .as_f64()
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
        .ok_or("Invalid Powerwall reading")?;
    let power_w = response["battery_power"]
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or("Invalid Powerwall reading")?;
    Ok(Powerwall {
        charge_pct,
        power_w,
    })
}

pub fn fetch_powerwall(
    client: &reqwest::blocking::Client,
    base: &str,
    site: u64,
    token: &str,
) -> Result<Powerwall, &'static str> {
    powerwall(&fetch_status(client, base, site, token)?)
}

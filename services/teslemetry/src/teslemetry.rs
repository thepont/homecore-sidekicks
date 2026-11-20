use serde_json::Value;
use std::io::Read;

pub fn load_kw(status: &Value) -> Result<f64, &'static str> {
    let watts = status["response"]["load_power"]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or("Invalid Teslemetry load reading")?;
    Ok(watts / 1000.0)
}

fn status_url(base: &str, site: u64, token: &str) -> Result<String, &'static str> {
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
    Ok(format!(
        "{}/api/1/energy_sites/{site}/live_status",
        base.trim_end_matches('/')
    ))
}

fn fetch_status(
    client: &reqwest::blocking::Client,
    base: &str,
    site: u64,
    token: &str,
) -> Result<Value, &'static str> {
    let url = status_url(base, site, token)?;
    let response = client
        .get(url)
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

pub fn fetch_energy(
    client: &reqwest::blocking::Client,
    base: &str,
    site: u64,
    token: &str,
) -> Result<(Powerwall, serde_json::Map<String, Value>), &'static str> {
    let status = fetch_status(client, base, site, token)?;
    energy(&status)
}

pub async fn fetch_energy_async(
    client: &reqwest::Client,
    base: &str,
    site: u64,
    token: &str,
) -> Result<(Powerwall, serde_json::Map<String, Value>), &'static str> {
    let url = status_url(base, site, token)?;
    let mut response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|_| "Teslemetry request failed")?;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Teslemetry request failed")?
    {
        if body.len() + chunk.len() > 65_536 {
            return Err("Invalid Teslemetry response");
        }
        body.extend_from_slice(&chunk);
    }
    let status = serde_json::from_slice(&body).map_err(|_| "Invalid Teslemetry response")?;
    energy(&status)
}

pub fn energy(status: &Value) -> Result<(Powerwall, serde_json::Map<String, Value>), &'static str> {
    let battery = powerwall(status)?;
    let response = &status["response"];
    let mut readings = serde_json::Map::new();
    for (source, target) in [
        ("solar_power", "solar_power_w"),
        ("grid_power", "grid_power_w"),
        ("load_power", "home_power_w"),
        ("generator_power", "generator_power_w"),
    ] {
        if let Some(value) = response.get(source).filter(|v| !v.is_null()) {
            let value = value
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("Invalid energy reading")?;
            readings.insert(target.into(), value.into());
        }
    }
    for name in ["grid_status", "island_status"] {
        if let Some(value) = response.get(name).filter(|v| !v.is_null()) {
            let value = value
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 128 && v.chars().all(|c| !c.is_control()))
                .ok_or("Invalid energy status")?;
            readings.insert(name.into(), value.into());
        }
    }
    if let Some(value) = response.get("storm_mode_active").filter(|v| !v.is_null()) {
        readings.insert(
            "storm_mode_active".into(),
            value.as_bool().ok_or("Invalid storm status")?.into(),
        );
    }
    if let Some(value) = response.get("wall_connectors").filter(|v| !v.is_null()) {
        let connectors = value
            .as_array()
            .filter(|v| v.len() <= 16)
            .ok_or("Invalid wall connectors")?;
        for (index, connector) in connectors.iter().enumerate() {
            if !connector.is_object() {
                return Err("Invalid wall connector");
            }
            for (source, suffix) in [
                ("wall_connector_power", "power_w"),
                ("wall_connector_state", "state"),
                ("wall_connector_fault_state", "fault_state"),
                ("ocpp_status", "ocpp_status"),
                ("powershare_session_state", "powershare_state"),
            ] {
                if let Some(value) = connector.get(source).filter(|v| !v.is_null()) {
                    let value = if suffix == "power_w" {
                        Value::from(
                            value
                                .as_f64()
                                .filter(|v| v.is_finite())
                                .ok_or("Invalid wall connector reading")?,
                        )
                    } else {
                        Value::from(value.as_i64().ok_or("Invalid wall connector status")?)
                    };
                    readings.insert(format!("wall_connector_{}_{suffix}", index + 1), value);
                }
            }
        }
    }
    Ok((battery, readings))
}

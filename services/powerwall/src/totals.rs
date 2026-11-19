//! Teslemetry's daily energy totals stream.
use serde_json::{Map, Value};
use std::time::Duration;
use tokio::sync::watch;

const FIELDS: &[&str] = &[
    "solar_energy_exported",
    "generator_energy_exported",
    "grid_energy_imported",
    "grid_services_energy_imported",
    "grid_services_energy_exported",
    "grid_energy_exported_from_solar",
    "grid_energy_exported_from_generator",
    "grid_energy_exported_from_battery",
    "battery_energy_exported",
    "battery_energy_imported_from_grid",
    "battery_energy_imported_from_solar",
    "battery_energy_imported_from_generator",
    "consumer_energy_imported_from_grid",
    "consumer_energy_imported_from_solar",
    "consumer_energy_imported_from_battery",
    "consumer_energy_imported_from_generator",
    "total_home_usage",
    "total_battery_charge",
    "total_battery_discharge",
    "total_solar_generation",
    "total_grid_energy_exported",
];

pub fn parse_event(data: &str, site: u64) -> Option<Map<String, Value>> {
    let data: Value = serde_json::from_str(data).ok()?;
    let id = data.get("id")?;
    let id = id.as_u64().or_else(|| id.as_str()?.parse().ok())?;
    if id != site {
        return None;
    }
    let date = data.get("date")?.as_str()?;
    let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).date_naive();
    if (date - today).num_days().unsigned_abs() > 1 {
        return None;
    }
    let totals = data.get("totals")?.as_object()?;
    let mut fields = Map::new();
    for name in FIELDS {
        if let Some(value) = totals.get(*name).filter(|v| !v.is_null()) {
            let value = value.as_f64().filter(|v| v.is_finite() && *v >= 0.0)?;
            fields.insert(format!("{name}_wh"), value.into());
        }
    }
    if fields.is_empty() {
        return None;
    }
    fields.insert("energy_totals_date".into(), date.to_string().into());
    Some(fields)
}

pub async fn listen(
    base: String,
    site: u64,
    token: String,
    sender: watch::Sender<Map<String, Value>>,
) {
    let Ok(url) = reqwest::Url::parse(&base) else {
        return;
    };
    if !(url.scheme() == "https" || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || site == 0
    {
        return;
    }
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(120))
        .build()
    {
        Ok(client) => client,
        Err(_) => return,
    };
    let mut latest = None;
    loop {
        let url = format!("{}/sse?topics=energy_totals", base.trim_end_matches('/'));
        if let Ok(response) = client.get(url).bearer_auth(&token).send().await {
            if let Ok(mut response) = response.error_for_status() {
                if !response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.starts_with("text/event-stream"))
                {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                let mut pending = Vec::new();
                let mut event = String::new();
                let mut data = String::new();
                while let Ok(Some(chunk)) = response.chunk().await {
                    pending.extend_from_slice(&chunk);
                    if pending.len() > 65_536 {
                        break;
                    }
                    while let Some(end) = pending.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = pending.drain(..=end).collect();
                        let Ok(line) = std::str::from_utf8(&line[..end]) else {
                            continue;
                        };
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        if line.is_empty() {
                            if event == "energy_totals" {
                                if let Some(fields) = parse_event(&data, site) {
                                    let created = serde_json::from_str::<Value>(&data)
                                        .ok()
                                        .and_then(|v| {
                                            v.get("createdAt")?.as_str().map(str::to_owned)
                                        })
                                        .and_then(|v| {
                                            chrono::DateTime::parse_from_rfc3339(&v).ok()
                                        });
                                    if let Some(created) = created {
                                        let now: chrono::DateTime<chrono::Utc> =
                                            std::time::SystemTime::now().into();
                                        if created <= now + chrono::Duration::minutes(2)
                                            && latest.is_none_or(|previous| created > previous)
                                        {
                                            latest = Some(created);
                                            let _ = sender.send(fields);
                                        }
                                    }
                                }
                            }
                            event.clear();
                            data.clear();
                        } else if let Some(value) = line.strip_prefix("event:") {
                            event = value.trim().to_owned();
                        } else if let Some(value) = line.strip_prefix("data:") {
                            if data.len() + value.len() > 65_536 {
                                data.clear();
                                event.clear();
                                break;
                            }
                            if !data.is_empty() {
                                data.push('\n');
                            }
                            data.push_str(value.strip_prefix(' ').unwrap_or(value));
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

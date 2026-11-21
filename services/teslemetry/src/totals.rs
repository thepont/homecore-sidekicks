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

fn nonnegative_finite(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn site_date(data: &Value, site: u64) -> Option<chrono::NaiveDate> {
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
    Some(date)
}

pub fn parse_event(data: &str, site: u64) -> Option<Map<String, Value>> {
    let data: Value = serde_json::from_str(data).ok()?;
    let date = site_date(&data, site)?;
    let totals = data.get("totals")?.as_object()?;
    let mut fields = Map::new();
    for name in FIELDS {
        if let Some(value) = totals.get(*name).filter(|v| !v.is_null()) {
            let value = nonnegative_finite(value)?;
            fields.insert(format!("{name}_wh"), value.into());
        }
    }
    if fields.is_empty() {
        return None;
    }
    fields.insert("energy_totals_date".into(), date.to_string().into());
    Some(fields)
}

pub fn parse_connector_totals(data: &str, site: u64) -> Option<Map<String, Value>> {
    let data: Value = serde_json::from_str(data).ok()?;
    let date = site_date(&data, site)?;
    let totals = data.get("totals")?.as_object()?;
    if totals.len() > 16 {
        return None;
    }
    let mut sum = 0.0;
    for value in totals.values() {
        sum += nonnegative_finite(value)?;
        if !sum.is_finite() {
            return None;
        }
    }
    let mut fields = Map::new();
    fields.insert("wall_connectors_energy_wh".into(), sum.into());
    fields.insert(
        "wall_connectors_energy_date".into(),
        date.to_string().into(),
    );
    Some(fields)
}

pub fn parse_site_info(data: &str, site: u64) -> Option<Map<String, Value>> {
    let data: Value = serde_json::from_str(data).ok()?;
    let id = data.get("site_id")?;
    if id.as_u64().or_else(|| id.as_str()?.parse().ok())? != site {
        return None;
    }
    let info = data.get("site_info")?.as_object()?;
    let mut fields = Map::new();
    for (source, target) in [
        ("backup_reserve_percent", "site_backup_reserve_pct"),
        ("vpp_backup_reserve_percent", "site_vpp_reserve_pct"),
        ("nameplate_energy", "site_nameplate_energy_wh"),
        ("nameplate_power", "site_nameplate_power_w"),
    ] {
        if let Some(value) = info.get(source).filter(|v| !v.is_null()) {
            let value = nonnegative_finite(value)?;
            if target.ends_with("_pct") && value > 100.0 {
                return None;
            }
            fields.insert(target.into(), value.into());
        }
    }
    if let Some(value) = info.get("battery_count").filter(|v| !v.is_null()) {
        fields.insert("site_battery_count".into(), value.as_u64()?.into());
    }
    for (source, target) in [
        ("default_real_mode", "site_operation_mode"),
        ("version", "site_version"),
    ] {
        if let Some(value) = info.get(source).filter(|v| !v.is_null()) {
            let value = value.as_str().filter(|v| {
                !v.is_empty() && v.len() <= 128 && v.chars().all(|c| !c.is_control())
            })?;
            fields.insert(target.into(), value.into());
        }
    }
    for (section, source, target) in [
        (
            "user_settings",
            "storm_mode_enabled",
            "site_storm_watch_enabled",
        ),
        (
            "user_settings",
            "off_grid_vehicle_charging_enabled",
            "site_off_grid_vehicle_charging_enabled",
        ),
        (
            "components",
            "grid_services_enabled",
            "site_grid_services_enabled",
        ),
        ("components", "solar", "site_has_solar"),
        ("components", "battery", "site_has_battery"),
        ("components", "grid", "site_has_grid"),
        ("components", "backup", "site_backup_capable"),
        (
            "components",
            "storm_mode_capable",
            "site_storm_watch_capable",
        ),
    ] {
        if let Some(value) = info
            .get(section)
            .and_then(|v| v.get(source))
            .filter(|v| !v.is_null())
        {
            fields.insert(target.into(), value.as_bool()?.into());
        }
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

/// Tariff and timezone snapshots from the site-scoped SSE connection.
#[derive(Clone, Debug, Default)]
pub struct TariffState {
    tariff: Option<crate::tariff::Tariff>,
    zone: Option<chrono_tz::Tz>,
    tariff_updated: Option<chrono::DateTime<chrono::FixedOffset>>,
    site_updated: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl TariffState {
    fn apply(&mut self, kind: &str, data: &str, site: u64) {
        if !matches!(kind, "site_info" | "tariff_content_v2") {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let Some(id) = value.get("site_id") else {
            return;
        };
        if id.as_u64().or_else(|| id.as_str()?.parse().ok()) != Some(site) {
            return;
        }
        let Some(observed) = value
            .get("createdAt")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        else {
            return;
        };
        let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
        if observed > now + chrono::Duration::minutes(2) {
            return;
        }
        if kind == "site_info" {
            if self
                .site_updated
                .is_some_and(|previous| observed < previous)
            {
                return;
            }
            self.site_updated = Some(observed);
            self.zone = value
                .get("site_info")
                .and_then(|info| info.get("installation_time_zone"))
                .and_then(Value::as_str)
                .and_then(|zone| zone.parse().ok());
        } else {
            if self
                .tariff_updated
                .is_some_and(|previous| observed < previous)
            {
                return;
            }
            self.tariff_updated = Some(observed);
            self.tariff = value
                .get("tariff_content_v2")
                .and_then(crate::tariff::parse);
        }
    }

    fn disconnected(&mut self) {
        self.tariff = None;
        self.zone = None;
    }

    /// Re-evaluate the schedule at publication time, even without a new event.
    pub fn readings(&self, now: chrono::DateTime<chrono::Utc>) -> Map<String, Value> {
        let mut fields = Map::new();
        let Some(updated) = self.tariff_updated else {
            return fields;
        };
        fields.insert("tariff_available".into(), false.into());
        fields.insert("tariff_updated_at".into(), updated.to_rfc3339().into());
        if let (Some(tariff), Some(zone)) = (&self.tariff, self.zone) {
            if let Some(prices) = tariff.readings(zone, now) {
                fields.extend(prices);
                fields.insert("tariff_available".into(), true.into());
            }
        }
        fields
    }
}

/// Keeps the last accepted timestamp for each totals topic across reconnects.
#[derive(Default)]
struct TotalsState {
    energy_updated: Option<chrono::DateTime<chrono::FixedOffset>>,
    connectors_updated: Option<chrono::DateTime<chrono::FixedOffset>>,
    site_updated: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl TotalsState {
    fn apply(
        &mut self,
        event: &str,
        data: &str,
        site: u64,
        sender: &watch::Sender<Map<String, Value>>,
    ) {
        let (fields, newest) = match event {
            "energy_totals" => (parse_event(data, site), &mut self.energy_updated),
            "wall_connector_totals" => (
                parse_connector_totals(data, site),
                &mut self.connectors_updated,
            ),
            "site_info" => (parse_site_info(data, site), &mut self.site_updated),
            _ => return,
        };
        let Some(fields) = fields else {
            return;
        };
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let Some(created) = value
            .get("createdAt")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        else {
            return;
        };
        let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
        if created <= now + chrono::Duration::minutes(2)
            && newest.is_none_or(|previous| created > previous)
        {
            *newest = Some(created);
            sender.send_modify(|state| state.extend(fields));
        }
    }
}

pub async fn listen(
    base: String,
    site: u64,
    token: String,
    sender: watch::Sender<Map<String, Value>>,
    tariffs: watch::Sender<TariffState>,
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
    let client = match crate::http::client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(120))
        .build()
    {
        Ok(client) => client,
        Err(_) => return,
    };
    let mut totals = TotalsState::default();
    loop {
        let url = format!(
            "{}/sse?topics=energy_totals,wall_connector_totals,site_info,tariff_content_v2",
            base.trim_end_matches('/')
        );
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
                            if matches!(event.as_str(), "site_info" | "tariff_content_v2") {
                                tariffs.send_modify(|state| state.apply(&event, &data, site));
                            }
                            totals.apply(&event, &data, site, &sender);
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
        tariffs.send_modify(TariffState::disconnected);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

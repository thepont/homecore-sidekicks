//! Validated Tesla V2 tariffs, resolved at the site's local time.
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Tariff {
    buy: Schedule,
    sell: Option<Schedule>,
    currency: String,
    name: Option<String>,
    provider: Option<String>,
}

#[derive(Clone, Debug)]
struct Schedule {
    seasons: Vec<Season>,
    charges: BTreeMap<String, BTreeMap<String, f64>>,
}

#[derive(Clone, Debug)]
struct Season {
    name: String,
    from: u32,
    to: u32,
    windows: Vec<Window>,
}

#[derive(Clone, Debug)]
struct Window {
    name: String,
    from_day: u32,
    to_day: u32,
    from_minute: u32,
    to_minute: u32,
}

fn text(value: &Value) -> Option<String> {
    let value = value.as_str()?;
    (!value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

fn optional_text(value: &Value, key: &str) -> Option<Option<String>> {
    match value.get(key) {
        None => Some(None),
        Some(value) => text(value).map(Some),
    }
}

fn number(value: &Value, key: &str, default: u32, max: u32) -> Option<u32> {
    let number = match value.get(key) {
        None => default,
        Some(value) => u32::try_from(value.as_u64()?).ok()?,
    };
    (number <= max).then_some(number)
}

fn month_day(value: &Value, prefix: &str) -> Option<u32> {
    let month = number(value, &format!("{prefix}Month"), 0, 12)?;
    let day = number(value, &format!("{prefix}Day"), 0, 31)?;
    NaiveDate::from_ymd_opt(2000, month, day)?;
    Some(month * 100 + day)
}

fn minute(value: &Value, prefix: &str) -> Option<u32> {
    let hour = number(value, &format!("{prefix}Hour"), 0, 24)?;
    let minute = number(value, &format!("{prefix}Minute"), 0, 60)?;
    let total = hour * 60 + minute;
    (total <= 1440 && (prefix == "to" || total < 1440)).then_some(total)
}

fn seasons(value: &Value) -> Option<Vec<Season>> {
    let values = value.as_object()?;
    if values.len() > 16 {
        return None;
    }
    let mut seasons = Vec::new();
    for (name, value) in values {
        if value.as_object()?.is_empty() {
            continue;
        }
        let groups = value.get("tou_periods")?.as_object()?;
        if groups.len() > 128 {
            return None;
        }
        let mut windows = Vec::new();
        for (period_name, group) in groups {
            let periods = group.get("periods")?.as_array()?;
            if periods.len() > 128 || windows.len() + periods.len() > 512 {
                return None;
            }
            for period in periods {
                period.as_object()?;
                windows.push(Window {
                    name: text(&Value::String(period_name.clone()))?,
                    from_day: number(period, "fromDayOfWeek", 0, 6)?,
                    to_day: number(period, "toDayOfWeek", 0, 6)?,
                    from_minute: minute(period, "from")?,
                    to_minute: minute(period, "to")?,
                });
            }
        }
        seasons.push(Season {
            name: text(&Value::String(name.clone()))?,
            from: month_day(value, "from")?,
            to: month_day(value, "to")?,
            windows,
        });
    }
    Some(seasons)
}

fn schedule(value: &Value, inherited_seasons: Option<&[Season]>) -> Option<Schedule> {
    let seasons = match value.get("seasons") {
        Some(value) => seasons(value)?,
        None => inherited_seasons.unwrap_or_default().to_vec(),
    };
    let values = value.get("energy_charges")?.as_object()?;
    if values.len() > 16 {
        return None;
    }
    let mut charges = BTreeMap::new();
    for (season_name, season) in values {
        season.as_object()?;
        let Some(rates) = season.get("rates") else {
            if season.as_object()?.is_empty() {
                continue;
            }
            return None;
        };
        let rates = rates.as_object()?;
        if rates.len() > 128 {
            return None;
        }
        let mut checked = BTreeMap::new();
        for (name, rate) in rates {
            checked.insert(
                text(&Value::String(name.clone()))?,
                rate.as_f64().filter(|v| v.is_finite())?,
            );
        }
        charges.insert(text(&Value::String(season_name.clone()))?, checked);
    }
    Some(Schedule { seasons, charges })
}

pub fn parse(value: &Value) -> Option<Tariff> {
    value.as_object()?;
    if value.get("version").is_some_and(|v| v.as_u64() != Some(1)) {
        return None;
    }
    let buy = schedule(value, None)?;
    let sell = match value.get("sell_tariff") {
        None => None,
        Some(value) => Some(schedule(value, Some(&buy.seasons))?),
    };
    let currency = match value.get("currency") {
        None => "unspecified".to_owned(),
        Some(value) => {
            let value = value.as_str()?;
            if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
                return None;
            }
            value.to_owned()
        }
    };
    Some(Tariff {
        buy,
        sell,
        currency,
        name: optional_text(value, "name")?,
        provider: optional_text(value, "utility")?,
    })
}

fn within(value: u32, from: u32, to: u32) -> bool {
    if from <= to {
        (from..=to).contains(&value)
    } else {
        value >= from || value <= to
    }
}

impl Window {
    fn contains(&self, start_weekday: u32, minute: u32, carry: bool) -> bool {
        if !within(start_weekday, self.from_day, self.to_day) {
            return false;
        }
        let overnight = self.to_minute <= self.from_minute;
        if carry {
            overnight && minute < self.to_minute
        } else {
            minute >= self.from_minute && (overnight || minute < self.to_minute)
        }
    }
}

impl Schedule {
    fn rate(&self, date: NaiveDate, minute: u32) -> Option<(f64, &str)> {
        if self.seasons.is_empty() {
            return self
                .charges
                .get("ALL")?
                .get("ALL")
                .map(|price| (*price, "ALL"));
        }
        let mut active = None;
        // An overnight window belongs to the date and season when it started.
        for (start_date, carry) in [(date, false), (date.pred_opt()?, true)] {
            let date_key = start_date.month() * 100 + start_date.day();
            let mut seasons = self
                .seasons
                .iter()
                .filter(|season| within(date_key, season.from, season.to));
            let Some(season) = seasons.next() else {
                continue;
            };
            if seasons.next().is_some() {
                return None;
            }
            if season.windows.is_empty()
                && !carry
                && active.replace((season.name.as_str(), "ALL")).is_some()
            {
                return None;
            }
            for window in &season.windows {
                if window.contains(start_date.weekday().num_days_from_monday(), minute, carry)
                    && active
                        .replace((season.name.as_str(), window.name.as_str()))
                        .is_some()
                {
                    return None;
                }
            }
        }
        let (season, period) = active?;
        let rates = self
            .charges
            .get(season)
            .or_else(|| self.charges.get("ALL"))?;
        let price = rates.get(period).or_else(|| rates.get("ALL"))?;
        Some((*price, period))
    }
}

impl Tariff {
    pub fn readings(&self, zone: Tz, now: DateTime<Utc>) -> Option<Map<String, Value>> {
        let local = now.with_timezone(&zone);
        let date = local.date_naive();
        let minute = local.hour() * 60 + local.minute();
        let (buy, buy_period) = self.buy.rate(date, minute)?;
        let mut fields = Map::new();
        fields.insert("tariff_buy_price_per_kwh".into(), buy.into());
        fields.insert("tariff_buy_period".into(), buy_period.into());
        let (currency, source, region) = if self.currency != "unspecified" {
            (self.currency.as_str(), "tariff", None)
        } else if let Some((currency, region)) = crate::currency::for_timezone(zone, date) {
            (currency, "site_timezone", Some(region))
        } else {
            ("unspecified", "unspecified", None)
        };
        fields.insert("tariff_currency".into(), currency.into());
        fields.insert("tariff_currency_source".into(), source.into());
        if let Some(region) = region {
            fields.insert("tariff_currency_region".into(), region.into());
        }
        if let Some((sell, sell_period)) =
            self.sell.as_ref().and_then(|sell| sell.rate(date, minute))
        {
            fields.insert("tariff_sell_price_per_kwh".into(), sell.into());
            fields.insert("tariff_sell_period".into(), sell_period.into());
        }
        if let Some(name) = &self.name {
            fields.insert("tariff_name".into(), name.clone().into());
        }
        if let Some(provider) = &self.provider {
            fields.insert("tariff_provider".into(), provider.clone().into());
        }
        Some(fields)
    }
}

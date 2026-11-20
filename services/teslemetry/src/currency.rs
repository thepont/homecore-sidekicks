//! Currency inference from geographic IANA zones and dated CLDR tender data.
use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;

include!("currency_data.rs");

/// Infer a currency from the zone's geographic territory on the site's local date.
/// A missing zone, absent currency or multiple legal tenders produces no inference.
pub fn for_timezone(zone: Tz, date: NaiveDate) -> Option<(&'static str, &'static str)> {
    let index = ZONES
        .binary_search_by_key(&zone.name(), |entry| entry.0)
        .ok()?;
    let region = ZONES[index].1;
    let day = u32::try_from(date.year()).ok()? * 10000 + date.month() * 100 + date.day();
    let mut active = CURRENCIES
        .iter()
        .filter(|entry| entry.0 == region && entry.2 <= day && day <= entry.3);
    let currency = active.next()?.1;
    active
        .all(|entry| entry.1 == currency)
        .then_some((currency, region))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(zone: &str, date: &str) -> Option<(&'static str, &'static str)> {
        for_timezone(zone.parse().unwrap(), date.parse().unwrap())
    }

    #[test]
    fn geographic_site_zone_provides_currency_without_a_machine_locale() {
        assert_eq!(
            lookup("Australia/Melbourne", "2026-11-20"),
            Some(("AUD", "AU"))
        );
    }

    #[test]
    fn geographic_identity_is_preserved_when_zones_share_clock_rules() {
        for (zone, currency, region) in [
            ("Europe/Zurich", "CHF", "CH"),
            ("Europe/Stockholm", "SEK", "SE"),
            ("Africa/Johannesburg", "ZAR", "ZA"),
            ("Pacific/Auckland", "NZD", "NZ"),
            ("America/New_York", "USD", "US"),
        ] {
            assert_eq!(
                lookup(zone, "2026-11-20"),
                Some((currency, region)),
                "{zone}"
            );
        }
    }

    #[test]
    fn unknown_and_non_geographic_zones_or_multiple_tenders_have_no_default() {
        for zone in [
            "UTC",
            "Etc/GMT-10",
            "US/Eastern",
            "Antarctica/Casey",
            "America/Panama",
            "Africa/Harare",
        ] {
            assert_eq!(lookup(zone, "2026-11-20"), None, "{zone}");
        }
    }

    #[test]
    fn tender_dates_are_inclusive_and_transition_overlap_is_ambiguous() {
        for (zone, date, expected) in [
            ("Australia/Melbourne", "1966-02-13", None),
            ("Australia/Melbourne", "1966-02-14", Some(("AUD", "AU"))),
            ("Europe/Sofia", "2025-12-31", Some(("BGN", "BG"))),
            ("Europe/Sofia", "2026-01-01", None),
            ("Europe/Sofia", "2026-01-31", None),
            ("Europe/Sofia", "2026-02-01", Some(("EUR", "BG"))),
            ("Europe/Zagreb", "2023-01-14", None),
            ("Europe/Zagreb", "2023-01-15", Some(("EUR", "HR"))),
        ] {
            assert_eq!(lookup(zone, date), expected, "{zone} {date}");
        }
    }
}

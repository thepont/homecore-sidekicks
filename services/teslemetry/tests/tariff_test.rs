use chrono::{DateTime, Utc};
use chrono_tz::Australia::Melbourne;
use homecore_teslemetry::tariff;
use serde_json::{json, Value};

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

// Shape and prices from a sanitised live tariff; no account identifiers.
fn live_tariff() -> Value {
    let seasons = json!({
        "Summer": {"fromDay":1,"fromMonth":1,"toDay":31,"toMonth":12,
            "tou_periods":{
                "ON_PEAK":{"periods":[{"fromHour":15,"toDayOfWeek":6,"toHour":21}]},
                "PARTIAL_PEAK":{"periods":[{"fromHour":21,"toDayOfWeek":6,"toHour":11}]},
                "SUPER_OFF_PEAK":{"periods":[{"fromHour":11,"toDayOfWeek":6,"toHour":15}]}
            }},
        "Winter":{}
    });
    json!({
        "version":1,"name":"Example plan","utility":"Example provider",
        "energy_charges":{"ALL":{"rates":{"ALL":0}},"Summer":{"rates":{
            "ON_PEAK":0.4906,"PARTIAL_PEAK":0.23276,"SUPER_OFF_PEAK":0}},"Winter":{}},
        "seasons":seasons,
        "sell_tariff":{
            "energy_charges":{"ALL":{"rates":{"ALL":0}},"Summer":{"rates":{
                "ON_PEAK":0.005,"PARTIAL_PEAK":0.005,"SUPER_OFF_PEAK":0.005}},"Winter":{}},
            "seasons":seasons
        }
    })
}

#[test]
fn actual_tariff_prices_follow_local_time_without_inventing_currency() {
    let tariff = tariff::parse(&live_tariff()).expect("live V2 tariff should be supported");
    for (utc, price, period) in [
        ("2026-11-20T03:59:59Z", 0.0, "SUPER_OFF_PEAK"),
        ("2026-11-20T04:00:00Z", 0.4906, "ON_PEAK"),
        ("2026-11-20T09:59:59Z", 0.4906, "ON_PEAK"),
        ("2026-11-20T10:00:00Z", 0.23276, "PARTIAL_PEAK"),
        ("2026-11-20T23:59:59Z", 0.23276, "PARTIAL_PEAK"),
        ("2026-11-21T00:00:00Z", 0.0, "SUPER_OFF_PEAK"),
    ] {
        let fields = tariff.readings(Melbourne, instant(utc)).unwrap();
        assert_eq!(fields["tariff_buy_price_per_kwh"], price, "{utc}");
        assert_eq!(fields["tariff_buy_period"], period, "{utc}");
        assert_eq!(fields["tariff_sell_price_per_kwh"], 0.005, "{utc}");
        assert_eq!(fields["tariff_currency"], "unspecified");
        assert_eq!(fields["tariff_name"], "Example plan");
        assert_eq!(fields["tariff_provider"], "Example provider");
    }
}

#[test]
fn flat_rates_keep_negative_and_zero_prices_and_missing_sell_is_not_zero() {
    let mut document = json!({"currency":"AUD","energy_charges":{"ALL":{"rates":{"ALL":-0.04}}}});
    let tariff = tariff::parse(&document).expect("a flat ALL tariff needs no time windows");
    let values = tariff
        .readings(Melbourne, instant("2026-11-20T04:00:00Z"))
        .unwrap();
    assert_eq!(values["tariff_buy_price_per_kwh"], -0.04);
    assert_eq!(values["tariff_buy_period"], "ALL");
    assert_eq!(values["tariff_currency"], "AUD");
    assert!(!values.contains_key("tariff_sell_price_per_kwh"));
    document["sell_tariff"] = json!({"energy_charges":{"ALL":{"rates":{"ALL":0}}}});
    let values = tariff::parse(&document)
        .unwrap()
        .readings(Melbourne, instant("2026-11-20T04:00:00Z"))
        .unwrap();
    assert_eq!(values["tariff_sell_price_per_kwh"], 0.0);
}

#[test]
fn overlapping_schedules_and_gaps_never_choose_an_arbitrary_price() {
    let now = instant("2026-11-20T05:00:00Z"); // Friday 16:00 in Melbourne.
    let mut overlap = live_tariff();
    overlap["seasons"]["Summer"]["tou_periods"]["SUPER_OFF_PEAK"]["periods"][0]["toHour"] =
        json!(17);
    assert!(tariff::parse(&overlap)
        .unwrap()
        .readings(Melbourne, now)
        .is_none());

    let mut duplicate_season = live_tariff();
    duplicate_season["seasons"]["AlsoSummer"] = duplicate_season["seasons"]["Summer"].clone();
    assert!(tariff::parse(&duplicate_season)
        .unwrap()
        .readings(Melbourne, now)
        .is_none());

    let mut gap = live_tariff();
    gap["seasons"]["Summer"]["tou_periods"]["ON_PEAK"]["periods"][0]["fromHour"] = json!(17);
    assert!(tariff::parse(&gap)
        .unwrap()
        .readings(Melbourne, now)
        .is_none());

    let mut unknown_price = live_tariff();
    unknown_price["energy_charges"]["Summer"]["rates"]
        .as_object_mut()
        .unwrap()
        .remove("ON_PEAK");
    assert!(tariff::parse(&unknown_price)
        .unwrap()
        .readings(Melbourne, now)
        .is_none());

    let mut ambiguous_sell = live_tariff();
    ambiguous_sell["sell_tariff"]["seasons"] = overlap["seasons"].clone();
    let values = tariff::parse(&ambiguous_sell)
        .unwrap()
        .readings(Melbourne, now)
        .unwrap();
    assert_eq!(values["tariff_buy_price_per_kwh"], 0.4906);
    assert!(!values.contains_key("tariff_sell_price_per_kwh"));
}

#[test]
fn malformed_tariffs_are_rejected_at_the_boundary() {
    let mut unknown_version = live_tariff();
    unknown_version["version"] = json!(99);
    assert!(tariff::parse(&unknown_version).is_none());
    for bad in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"energy_charges":null}),
    ] {
        assert!(tariff::parse(&bad).is_none());
    }
    for (pointer, bad) in [
        ("/currency", json!("aud")),
        ("/currency", json!("AUD/kWh")),
        ("/name", json!("invalid\nname")),
        ("/name", json!("x".repeat(129))),
        ("/energy_charges/Summer/rates/ON_PEAK", json!("0.49")),
        ("/energy_charges/Summer/rates/ON_PEAK", Value::Null),
        ("/seasons/Summer/fromMonth", json!(13)),
        ("/seasons/Summer/fromDay", json!(0)),
        (
            "/seasons/Summer/tou_periods/ON_PEAK/periods/0/toHour",
            json!(25),
        ),
        (
            "/seasons/Summer/tou_periods/ON_PEAK/periods/0/toMinute",
            json!(61),
        ),
        (
            "/seasons/Summer/tou_periods/ON_PEAK/periods/0/toDayOfWeek",
            json!(7),
        ),
        (
            "/seasons/Summer/tou_periods/ON_PEAK/periods",
            json!(vec![json!({}); 129]),
        ),
    ] {
        let mut document = live_tariff();
        if pointer == "/currency" {
            document["currency"] = bad;
        } else if pointer.ends_with("toMinute") {
            document["seasons"]["Summer"]["tou_periods"]["ON_PEAK"]["periods"][0]["toMinute"] = bad;
        } else {
            *document.pointer_mut(pointer).unwrap() = bad;
        }
        assert!(tariff::parse(&document).is_none(), "accepted {pointer}");
    }
}

#[test]
fn calendar_resolution_observes_seasons_weekdays_and_dst() {
    let seasonal = json!({
        "currency":"AUD",
        "energy_charges":{"Warm":{"rates":{"ALL":0.61}},"Cool":{"rates":{"ALL":0.19}}},
        "seasons":{
            "Warm":{"fromMonth":10,"fromDay":1,"toMonth":3,"toDay":31,"tou_periods":{"DAY":{"periods":[{"toDayOfWeek":6,"toHour":24}]}}},
            "Cool":{"fromMonth":4,"fromDay":1,"toMonth":9,"toDay":30,"tou_periods":{"DAY":{"periods":[{"toDayOfWeek":6,"toHour":24}]}}}
        }
    });
    let tariff = tariff::parse(&seasonal).unwrap();
    for (utc, price) in [
        ("2026-03-31T12:59:59Z", 0.61),
        ("2026-03-31T13:00:00Z", 0.19),
        ("2026-09-30T13:59:59Z", 0.19),
        ("2026-09-30T14:00:00Z", 0.61),
    ] {
        assert_eq!(
            tariff.readings(Melbourne, instant(utc)).unwrap()["tariff_buy_price_per_kwh"],
            price,
            "{utc}"
        );
    }

    let friday_night = json!({"energy_charges":{"ALL":{"rates":{"NIGHT":0.17}}},"seasons":{"Year":{
        "fromMonth":1,"fromDay":1,"toMonth":12,"toDay":31,
        "tou_periods":{"NIGHT":{"periods":[{"fromDayOfWeek":4,"toDayOfWeek":4,"fromHour":22,"toHour":6}]}}
    }}});
    let night = tariff::parse(&friday_night).unwrap();
    assert_eq!(
        night
            .readings(Melbourne, instant("2026-11-20T18:00:00Z"))
            .unwrap()["tariff_buy_price_per_kwh"],
        0.17
    ); // Saturday 05:00, Friday's window.
    assert!(night
        .readings(Melbourne, instant("2026-11-20T19:00:00Z"))
        .is_none()); // Saturday 06:00.
    assert!(night
        .readings(Melbourne, instant("2026-11-19T18:00:00Z"))
        .is_none()); // Friday 05:00 is not Thursday's window.

    let mut wrapped_week = friday_night.clone();
    wrapped_week["seasons"]["Year"]["tou_periods"]["NIGHT"]["periods"][0] =
        json!({"fromDayOfWeek":4,"toDayOfWeek":0,"toHour":24});
    let weekend = tariff::parse(&wrapped_week).unwrap();
    assert!(weekend
        .readings(Melbourne, instant("2026-11-22T00:00:00Z"))
        .is_some()); // Sunday.
    assert!(weekend
        .readings(Melbourne, instant("2026-11-24T00:00:00Z"))
        .is_none()); // Tuesday.

    let tariff = tariff::parse(&live_tariff()).unwrap();
    for (utc, price) in [
        ("2026-10-03T04:59:59Z", 0.0),
        ("2026-10-03T05:00:00Z", 0.4906), // 15:00 before daylight saving.
        ("2026-10-04T03:59:59Z", 0.0),
        ("2026-10-04T04:00:00Z", 0.4906), // 15:00 after daylight saving.
    ] {
        assert_eq!(
            tariff.readings(Melbourne, instant(utc)).unwrap()["tariff_buy_price_per_kwh"],
            price,
            "{utc}"
        );
    }
}

#[test]
fn rate_fallback_is_not_added_and_sell_can_have_its_own_schedule() {
    let mut document = live_tariff();
    document["currency"] = json!("AUD");
    document["energy_charges"]["ALL"]["rates"]["ALL"] = json!(0.08);
    document["sell_tariff"]
        .as_object_mut()
        .unwrap()
        .remove("seasons");
    let fields = tariff::parse(&document)
        .unwrap()
        .readings(Melbourne, instant("2026-11-20T05:00:00Z"))
        .unwrap();
    assert_eq!(fields["tariff_buy_price_per_kwh"], 0.4906); // Not 0.5706.
    assert_eq!(fields["tariff_sell_price_per_kwh"], 0.005); // Inherited buy geometry.

    document["energy_charges"]["Summer"] = json!({});
    let fields = tariff::parse(&document)
        .unwrap()
        .readings(Melbourne, instant("2026-11-20T05:00:00Z"))
        .unwrap();
    assert_eq!(fields["tariff_buy_price_per_kwh"], 0.08);
    document["energy_charges"]["Summer"] = json!({"rates":{}});
    assert!(tariff::parse(&document)
        .unwrap()
        .readings(Melbourne, instant("2026-11-20T05:00:00Z"))
        .is_none());

    let mut document = live_tariff();
    document["sell_tariff"]["seasons"]["Summer"]["tou_periods"] =
        json!({"EXPORT":{"periods":[{"fromHour":16,"toHour":16,"toMinute":60,"toDayOfWeek":6}]}});
    document["sell_tariff"]["energy_charges"]["Summer"]["rates"] = json!({"EXPORT":0.34});
    let tariff = tariff::parse(&document).unwrap();
    let before = tariff
        .readings(Melbourne, instant("2026-11-20T04:59:59Z"))
        .unwrap();
    assert!(!before.contains_key("tariff_sell_price_per_kwh"));
    let during = tariff
        .readings(Melbourne, instant("2026-11-20T05:00:00Z"))
        .unwrap();
    assert_eq!(during["tariff_sell_price_per_kwh"], 0.34);
    assert_eq!(during["tariff_sell_period"], "EXPORT");
    assert_eq!(during["tariff_buy_period"], "ON_PEAK");
    let after = tariff
        .readings(Melbourne, instant("2026-11-20T06:00:00Z"))
        .unwrap();
    assert!(!after.contains_key("tariff_sell_price_per_kwh"));
}

#[test]
fn overnight_window_keeps_the_season_of_its_start_day() {
    let windows = json!({
        "NIGHT":{"periods":[{"fromHour":22,"toHour":6,"toDayOfWeek":6}]},
        "DAY":{"periods":[{"fromHour":6,"toHour":22,"toDayOfWeek":6}]}
    });
    let tariff = tariff::parse(&json!({
        "energy_charges":{"First":{"rates":{"NIGHT":0.7,"DAY":0.1}},"Second":{"rates":{"NIGHT":0.2,"DAY":0.15}}},
        "seasons":{
            "First":{"fromMonth":1,"fromDay":1,"toMonth":6,"toDay":30,"tou_periods":windows},
            "Second":{"fromMonth":7,"fromDay":1,"toMonth":12,"toDay":31,"tou_periods":windows}
        }
    })).unwrap();
    assert_eq!(
        tariff
            .readings(Melbourne, instant("2026-06-30T18:00:00Z"))
            .unwrap()["tariff_buy_price_per_kwh"],
        0.7
    ); // July 1, 04:00: June 30's window.
    assert_eq!(
        tariff
            .readings(Melbourne, instant("2026-06-30T20:00:00Z"))
            .unwrap()["tariff_buy_price_per_kwh"],
        0.15
    ); // July 1, 06:00.
    assert_eq!(
        tariff
            .readings(Melbourne, instant("2026-07-01T13:00:00Z"))
            .unwrap()["tariff_buy_price_per_kwh"],
        0.2
    ); // July 1, 23:00.
}

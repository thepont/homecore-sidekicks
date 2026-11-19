use std::{env, fs};

#[test]
#[ignore = "requires a private Teslemetry credential and network; read-only"]
fn powerwall_adapter_agrees_with_the_live_tesla_source() {
    let token = fs::read_to_string(env::var("TESLEMETRY_TOKEN_FILE").unwrap()).unwrap();
    let site: u64 = fs::read_to_string(env::var("TESLEMETRY_SITE_ID_FILE").unwrap())
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let client = homecore_powerwall::http::blocking_client().unwrap();
    let direct: serde_json::Value = client
        .get(format!(
            "https://api.teslemetry.com/api/1/energy_sites/{site}/live_status"
        ))
        .bearer_auth(token.trim())
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    let expected = direct["response"]["percentage_charged"].as_f64().unwrap();
    let source_power = direct["response"]["battery_power"].as_f64().unwrap();
    let actual = homecore_powerwall::teslemetry::fetch_powerwall(
        &client,
        "https://api.teslemetry.com",
        site,
        token.trim(),
    )
    .unwrap();
    assert!(
        (actual.charge_pct - expected).abs() < 0.5,
        "charge readings differ"
    );
    assert!(
        (actual.power_w - source_power).abs() < 500.0,
        "power readings differ by at least 500 W between samples"
    );
    let (_, energy) = homecore_powerwall::teslemetry::fetch_energy(
        &client,
        "https://api.teslemetry.com",
        site,
        token.trim(),
    )
    .unwrap();
    for (source, target) in [
        ("solar_power", "solar_power_w"),
        ("grid_power", "grid_power_w"),
        ("load_power", "home_power_w"),
        ("generator_power", "generator_power_w"),
    ] {
        let expected = direct["response"][source].as_f64().unwrap();
        let actual = energy[target].as_f64().unwrap();
        assert!(
            (actual - expected).abs() < 1000.0,
            "{target} differs between samples"
        );
    }
}

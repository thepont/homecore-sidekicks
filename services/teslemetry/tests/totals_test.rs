use serde_json::json;

#[test]
fn energy_totals_are_scoped_dated_and_validated() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = json!({"id":"42", "date":now.format("%Y-%m-%d").to_string(),
        "totals":{"total_solar_generation":1234,"grid_energy_imported":87}});
    let fields = homecore_teslemetry::totals::parse_event(&event.to_string(), 42).unwrap();
    assert_eq!(fields["total_solar_generation_wh"], 1234.0);
    assert_eq!(fields["grid_energy_imported_wh"], 87.0);
    assert_eq!(
        fields["energy_totals_date"],
        now.format("%Y-%m-%d").to_string()
    );
    assert!(homecore_teslemetry::totals::parse_event(&event.to_string(), 43).is_none());
    let mut bad = event.clone();
    bad["date"] = json!("2000-01-01");
    assert!(homecore_teslemetry::totals::parse_event(&bad.to_string(), 42).is_none());
    let mut bad = event.clone();
    bad["totals"]["grid_energy_imported"] = json!(-1);
    assert!(homecore_teslemetry::totals::parse_event(&bad.to_string(), 42).is_none());
    assert!(homecore_teslemetry::totals::parse_event("not json", 42).is_none());
}

#[test]
fn site_configuration_is_scoped_and_rejects_invalid_reserve() {
    let event = json!({"site_id":"42","site_info":{"backup_reserve_percent":22.5,
        "default_real_mode":"self_consumption","nameplate_power":5000,"battery_count":1,
        "components":{"solar":true,"grid_services_enabled":false}}});
    let fields = homecore_teslemetry::totals::parse_site_info(&event.to_string(), 42).unwrap();
    assert_eq!(fields["site_backup_reserve_pct"], 22.5);
    assert_eq!(fields["site_has_solar"], true);
    assert_eq!(fields["site_nameplate_power_w"], 5000.0);
    assert!(homecore_teslemetry::totals::parse_site_info(&event.to_string(), 43).is_none());
    let mut bad = event.clone();
    bad["site_info"]["backup_reserve_percent"] = json!(101);
    assert!(homecore_teslemetry::totals::parse_site_info(&bad.to_string(), 42).is_none());
}

#[test]
fn charger_totals_are_site_scoped_and_dated() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = json!({"id":"42", "date":now.format("%Y-%m-%d").to_string(), "totals":{"fixture-1":1200,"fixture-2":800}});
    let result =
        homecore_teslemetry::totals::parse_connector_totals(&event.to_string(), 42).unwrap();
    assert_eq!(result["wall_connectors_energy_wh"], 2000.0);
    assert_eq!(
        result["wall_connectors_energy_date"],
        now.format("%Y-%m-%d").to_string()
    );
    assert!(homecore_teslemetry::totals::parse_connector_totals(&event.to_string(), 43).is_none());
    let mut old = event.clone();
    old["date"] = json!("2000-01-01");
    assert!(homecore_teslemetry::totals::parse_connector_totals(&old.to_string(), 42).is_none());
    let mut invalid = event.clone();
    invalid["totals"]["fixture-1"] = json!(-1);
    assert!(
        homecore_teslemetry::totals::parse_connector_totals(&invalid.to_string(), 42).is_none()
    );
}

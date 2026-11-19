use serde_json::json;

#[test]
fn energy_totals_are_scoped_dated_and_validated() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let event = json!({"id":"42", "date":now.format("%Y-%m-%d").to_string(),
        "totals":{"total_solar_generation":1234,"grid_energy_imported":87}});
    let fields = homecore_powerwall::totals::parse_event(&event.to_string(), 42).unwrap();
    assert_eq!(fields["total_solar_generation_wh"], 1234.0);
    assert_eq!(fields["grid_energy_imported_wh"], 87.0);
    assert_eq!(
        fields["energy_totals_date"],
        now.format("%Y-%m-%d").to_string()
    );
    assert!(homecore_powerwall::totals::parse_event(&event.to_string(), 43).is_none());
    let mut bad = event.clone();
    bad["date"] = json!("2000-01-01");
    assert!(homecore_powerwall::totals::parse_event(&bad.to_string(), 42).is_none());
    let mut bad = event.clone();
    bad["totals"]["grid_energy_imported"] = json!(-1);
    assert!(homecore_powerwall::totals::parse_event(&bad.to_string(), 42).is_none());
    assert!(homecore_powerwall::totals::parse_event("not json", 42).is_none());
}

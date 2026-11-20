use homecore_electrolux::parse_status;
use serde_json::json;

#[test]
fn reads_electrolux_status_into_climate_values() {
    let state = parse_status(
        &json!({"envtemp": 22, "temp": 24, "ac_pwr": 1, "ac_mode": 0, "ac_mark": 3, "ac_vdir": 1}),
    )
    .unwrap();
    assert_eq!(state.current_temperature, 22.0);
    assert_eq!(state.target_temperature, 24.0);
    assert_eq!(state.mode, "cool");
    assert_eq!(state.fan, "high");
    assert!(state.swing);
}

#[test]
fn rejects_missing_invalid_and_out_of_range_values() {
    let valid =
        json!({"envtemp": 22, "temp": 24, "ac_pwr": 1, "ac_mode": 0, "ac_mark": 3, "ac_vdir": 1});
    for (field, value) in [
        ("envtemp", json!("hot")),
        ("temp", json!(41)),
        ("ac_pwr", json!(2)),
        ("ac_mode", json!(99)),
        ("ac_mark", json!(-1)),
        ("ac_vdir", json!(3)),
    ] {
        let mut state = valid.clone();
        state[field] = value;
        assert!(parse_status(&state).is_err(), "{field}");
    }
    assert!(parse_status(&json!({})).is_err());
}

#[test]
fn off_state_keeps_reported_fan_and_swing() {
    let state = parse_status(&json!({"envtemp": 18.5, "temp": 21, "ac_pwr": 0, "ac_mode": 1, "ac_mark": 0, "ac_vdir": 0})).unwrap();
    assert_eq!(state.mode, "off");
    assert_eq!(state.fan, "auto");
    assert!(!state.swing);
}

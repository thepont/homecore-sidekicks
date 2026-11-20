use serde_json::json;

#[test]
fn malformed_vehicle_readings_do_not_become_sensors() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let event = json!({"vin":vin,"createdAt":now.to_rfc3339(),"data":{"BatteryLevel":101,"Locked":true,"Location":{"latitude":91,"longitude":181}}});
    let (_, fields) = homecore_powerwall::vehicle::parse_event(&event.to_string(), vin).unwrap();
    assert!(!fields.contains_key("battery_level"));
    assert!(!fields.contains_key("location_latitude"));
    assert!(!fields.contains_key("location_longitude"));
}

#[test]
fn vehicle_events_are_scoped_and_never_include_identifiers() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let vin = "5YJ3E1EA7KF000001";
    let event = json!({"vin":vin,"createdAt":now.to_rfc3339(),"data":{"BatteryLevel":62,"Locked":true,"Location":{"latitude":-37.8,"longitude":144.9},"Vin":vin,"UserId":"private-id","Driving":true}});
    let (_, fields) = homecore_powerwall::vehicle::parse_event(&event.to_string(), vin).unwrap();
    assert_eq!(fields["battery_level"], 62);
    assert_eq!(fields["locked"], true);
    assert_eq!(fields["location_latitude"], -37.8);
    assert_eq!(fields["driving"], true);
    let serialized = serde_json::to_string(&fields).unwrap();
    assert!(!serialized.contains(vin));
    assert!(!serialized.contains("private-id"));
    assert!(
        homecore_powerwall::vehicle::parse_event(&event.to_string(), "5YJ3E1EA7KF000002").is_none()
    );
    let mut future = event;
    future["createdAt"] = json!((now + chrono::Duration::hours(1)).to_rfc3339());
    assert!(homecore_powerwall::vehicle::parse_event(&future.to_string(), vin).is_none());
}

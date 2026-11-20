use homecore_homekit::contact_state;
use serde_json::json;

#[test]
fn contact_sensor_projects_open_state_and_battery_into_homecore() {
    let accessory = json!({"aid":7,"services":[
        {"type":"80","characteristics":[{"iid":10,"type":"6A","value":1}]},
        {"type":"96","characteristics":[{"iid":15,"type":"68","value":69}]}
    ]});
    assert_eq!(
        contact_state(&accessory),
        Some(json!({"open":true,"battery_pct":69}))
    );
}

#[test]
fn invalid_or_ambiguous_contact_readings_are_rejected() {
    let contact = |value| {
        json!({"services":[
            {"type":"80","characteristics":[{"type":"6A","value":value}]}
        ]})
    };
    assert_eq!(
        contact_state(&contact(json!(0))),
        Some(json!({"open":false}))
    );
    assert_eq!(contact_state(&contact(json!(2))), None);
    assert_eq!(contact_state(&contact(json!("1"))), None);
    assert_eq!(contact_state(&json!({"services":[]})), None);
    assert_eq!(
        contact_state(&json!({"services":[
            {"type":"80","characteristics":[{"type":"6A","value":0},{"type":"6A","value":1}]}
        ]})),
        None
    );
    assert_eq!(
        contact_state(&json!({"services":[
            {"type":"80","characteristics":[{"type":"6A","value":0}]},
            {"type":"96","characteristics":[{"type":"68","value":101}]}
        ]})),
        None
    );
}

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

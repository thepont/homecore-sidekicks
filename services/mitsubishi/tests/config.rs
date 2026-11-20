use homecore_mitsubishi::config::Config;
use serde_json::json;

#[test]
fn two_explicit_units_receive_bounded_default_timing() {
    let config = Config::parse(
        &serde_json::to_vec(&json!({"units":[
            {"id":"dining", "name":"Dining AC", "url":"ws://192.0.2.11/ws"},
            {"id":"bedroom_2", "name":"Bedroom AC", "url":"ws://unit-2.local/ws"}
        ]}))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(config.units.len(), 2);
    assert_eq!(config.units[0].id, "dining");
    assert_eq!(config.units[1].url, "ws://unit-2.local/ws");
    assert_eq!(config.publish_interval_secs, 5);
    assert_eq!(config.stale_after_secs, 10);
}

#[test]
fn invalid_endpoints_never_reach_the_transport() {
    for url in [
        "ws://unit.local:65536/ws",
        "ws://unit.local:0/ws",
        "http://unit.local/ws",
        "ws://user:secret@unit.local/ws",
        "ws://unit.local/ws?command=reset",
        "ws://unit.local/ws#fragment",
        "ws://unit.local/",
        "ws://unit.local:abc/ws",
        "ws://:80/ws",
        "ws://[invalid]/ws",
        "ws://unit..local/ws",
    ] {
        let data = json!({"units":[{"id":"room", "name":"Room", "url":url}]});
        assert!(
            Config::parse(&serde_json::to_vec(&data).unwrap()).is_err(),
            "{url}"
        );
    }
}

#[test]
fn ambiguous_identity_unbounded_lists_and_mistyped_settings_are_rejected() {
    let unit = json!({"id":"room", "name":"Room", "url":"ws://unit.local/ws"});
    let mut cases = vec![
        json!(null),
        json!({"units":[]}),
        json!({"units":vec![unit.clone();33]}),
        json!({"units":[unit.clone(),unit.clone()]}),
        json!({"units":[unit.clone()],"publish_interval_secs":0}),
        json!({"units":[unit.clone()],"publish_interval_secs":61}),
        json!({"units":[unit.clone()],"stale_after_secs":1}),
        json!({"units":[unit.clone()],"stale_after_secs":301}),
        json!({"units":[unit.clone()],"stale_after_secs":"10"}),
        json!({"units":[unit.clone()],"poll_interval_secs":5}),
    ];
    for id in ["Room", "../room", "room/other", "room#", "1room", ""] {
        let mut invalid = unit.clone();
        invalid["id"] = id.into();
        cases.push(json!({"units":[invalid]}));
    }
    let mut duplicate = unit.clone();
    duplicate["id"] = "second".into();
    duplicate["url"] = "ws://UNIT.local:80/ws".into();
    cases.push(json!({"units":[unit.clone(),duplicate]}));
    let mut secret = unit.clone();
    secret["password"] = "private fixture".into();
    cases.push(json!({"units":[secret]}));
    for data in cases {
        assert!(Config::parse(&serde_json::to_vec(&data).unwrap()).is_err());
    }
    assert!(Config::parse(&vec![b' '; 65537]).is_err());
}

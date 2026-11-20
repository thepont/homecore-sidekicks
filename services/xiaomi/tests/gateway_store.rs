use homecore_xiaomi::store::{GatewayConfig, Store};
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn gateway_configuration_survives_restart_as_one_private_record() {
    let root = std::env::temp_dir().join(format!("xiaomi-config-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let config = GatewayConfig::new(
        "192.168.1.12",
        "lumi.gateway.mgl03",
        "0123456789abcdef0123456789abcdef",
    )
    .unwrap();
    Store::new(&root).save_gateway(&config).unwrap();
    let restored = Store::new(&root).load_gateway().unwrap().unwrap();
    assert_eq!(restored.address(), "192.168.1.12");
    assert_eq!(restored.model(), "lumi.gateway.mgl03");
    assert_eq!(restored.token(), "0123456789abcdef0123456789abcdef");
    assert_eq!(
        fs::metadata(root.join("gateway.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(GatewayConfig::new(
        "8.8.8.8",
        "lumi.gateway.mgl03",
        "0123456789abcdef0123456789abcdef"
    )
    .is_err());
    fs::remove_dir_all(root).unwrap();
}

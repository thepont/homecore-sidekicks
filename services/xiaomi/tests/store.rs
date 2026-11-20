use homecore_xiaomi::store::Store;
use std::{fs, os::unix::fs::PermissionsExt};
#[test]
fn gateway_token_survives_restarting_with_private_permissions() {
    let root = std::env::temp_dir().join(format!("xiaomi-store-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let s = Store::new(&root);
    assert_eq!(s.load().unwrap(), None);
    s.save("0123456789abcdef0123456789abcdef").unwrap();
    let s = Store::new(&root);
    assert_eq!(
        s.load().unwrap().as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    assert_eq!(
        fs::metadata(root.join("gateway-token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(s.save("not-a-token").is_err());
    assert_eq!(
        s.load().unwrap().as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    fs::remove_file(root.join("gateway-token")).unwrap();
    fs::remove_dir(root).unwrap();
}

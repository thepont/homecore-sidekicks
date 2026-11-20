use homecore_xiaomi::cloud::legacy_encrypt;
#[test]
fn xiaomi_legacy_cipher_matches_independent_openssl_vector() {
    let mut key = [0u8; 32];
    for (i, b) in key.iter_mut().enumerate() {
        *b = i as u8;
    }
    assert_eq!(
        legacy_encrypt(&key, b"Plaintext"),
        vec![0xe6, 0x04, 0x31, 0x85, 0x5e, 0x87, 0x99, 0x83, 0x2b]
    );
}

#[test]
fn signs_cloud_request_against_independent_reference_vector() {
    let signed = homecore_xiaomi::cloud::sign_request(
        "/v2/homeroom/gethome",
        "{\"limit\":300}",
        "Zml4dHVyZS1zZWN1cml0eSE=",
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    )
    .unwrap();
    assert_eq!(signed.data, "E4q0N+hACx3zw8SqIQ==");
    assert_eq!(signed.hash, "IOnrdeRCBW/7m5v4HzVzg8q1IYqTDE2+Lt9HyA==");
    assert_eq!(signed.signature, "INVa0aSH7kzb8WeAKvM+OJRaYXk=");
    assert!(homecore_xiaomi::cloud::sign_request(
        "/invalid",
        "{}",
        "Zml4dHVyZS1zZWN1cml0eSE=",
        &[0; 12]
    )
    .is_err());
}

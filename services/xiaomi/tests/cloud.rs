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

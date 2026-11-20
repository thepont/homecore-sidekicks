use rc4::{KeyInit, Rc4, StreamCipher};

/// Xiaomi's cloud protocol uses RC4-drop1024 inside TLS for legacy compatibility.
pub fn legacy_encrypt(key: &[u8; 32], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::new_from_slice(key).expect("32-byte key is valid");
    let mut discard = [0u8; 1024];
    cipher.apply_keystream(&mut discard);
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

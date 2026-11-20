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

use base64::{engine::general_purpose::STANDARD, Engine};
use ring::digest::{digest, SHA1_FOR_LEGACY_USE_ONLY, SHA256};
pub struct SignedRequest {
    pub data: String,
    pub hash: String,
    pub signature: String,
}
pub fn sign_request(
    path: &str,
    data: &str,
    security: &str,
    nonce: &[u8; 12],
) -> Result<SignedRequest, &'static str> {
    if !matches!(
        path,
        "/v2/homeroom/gethome" | "/v2/home/home_device_list" | "/v2/user/get_device_cnt"
    ) || data.len() > 8192
    {
        return Err("Invalid cloud request");
    }
    let security = STANDARD
        .decode(security)
        .map_err(|_| "Invalid cloud session")?;
    if security.is_empty() || security.len() > 128 {
        return Err("Invalid cloud session");
    }
    let mut material = security;
    material.extend_from_slice(nonce);
    let hash = digest(&SHA256, &material);
    let key: [u8; 32] = hash
        .as_ref()
        .try_into()
        .map_err(|_| "Invalid cloud session")?;
    let signed = STANDARD.encode(key);
    let sha = |value: String| {
        STANDARD.encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, value.as_bytes()).as_ref())
    };
    let raw = sha(format!("POST&{path}&data={data}&{signed}"));
    let enc = STANDARD.encode(legacy_encrypt(&key, data.as_bytes()));
    let enc_hash = STANDARD.encode(legacy_encrypt(&key, raw.as_bytes()));
    let signature = sha(format!(
        "POST&{path}&data={enc}&rc4_hash__={enc_hash}&{signed}"
    ));
    Ok(SignedRequest {
        data: enc,
        hash: enc_hash,
        signature,
    })
}

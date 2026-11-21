use crate::{
    adapters::valid_identifier,
    login::DiscoveredDevice,
    transport::{read_key_file, valid_region, KeyFileError},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{json, Value};
use std::path::Path;
use zeroize::Zeroizing;

const INVALID: &str = "Invalid Xiaomi credential record";
const ENVELOPE_BYTES: usize = 12 + 4 + 32 + 16;

pub(crate) struct Vault(LessSafeKey);

pub(crate) fn valid_device_ref(reference: &str) -> bool {
    reference
        .split_once(':')
        .is_some_and(|(region, id)| valid_region(region) && valid_identifier(id))
}

impl Vault {
    pub(crate) fn from_file(path: &Path) -> Result<Self, &'static str> {
        let key = read_key_file(path).map_err(|error| match error {
            KeyFileError::Read => "Cannot read Xiaomi vault key",
            KeyFileError::InvalidLength => "Xiaomi vault key must be 32 bytes",
        })?;
        Ok(Self(LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &*key).map_err(|_| "Invalid Xiaomi vault key")?,
        )))
    }

    pub(crate) fn seal(
        &self,
        region: &str,
        discovery: &DiscoveredDevice,
    ) -> Result<StoredDevice, &'static str> {
        let credential = discovery.credential.as_ref().ok_or(INVALID)?;
        let reference = format!("{region}:{}", discovery.device.id);
        let aad = association(&reference, &discovery.device.model);
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| "Cannot seal Xiaomi credential")?;
        let mut payload = Zeroizing::new(Vec::with_capacity(ENVELOPE_BYTES - 12));
        payload.extend_from_slice(&credential.address.octets());
        payload.extend_from_slice(credential.token.as_bytes());
        self.0
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad.as_bytes()),
                &mut *payload,
            )
            .map_err(|_| "Cannot seal Xiaomi credential")?;
        let mut envelope = [0u8; ENVELOPE_BYTES];
        envelope[..12].copy_from_slice(&nonce);
        envelope[12..].copy_from_slice(&payload);
        Ok(StoredDevice {
            reference,
            model: discovery.device.model.clone(),
            name: discovery.device.name.clone(),
            sealed: STANDARD.encode(envelope),
        })
    }

    pub(crate) fn verify(&self, device: &StoredDevice) -> Result<(), &'static str> {
        let mut envelope = Zeroizing::new(decode(&device.sealed)?);
        let (nonce, body) = envelope.split_at_mut(12);
        let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| INVALID)?;
        let aad = association(&device.reference, &device.model);
        self.0
            .open_in_place(nonce, Aad::from(aad.as_bytes()), body)
            .map_err(|_| INVALID)?;
        Ok(())
    }
}

fn association(reference: &str, model: &str) -> String {
    format!("homecore-xiaomi/device-v1:{reference}:{model}")
}

fn decode(sealed: &str) -> Result<[u8; ENVELOPE_BYTES], &'static str> {
    if sealed.len() != 88 {
        return Err(INVALID);
    }
    STANDARD
        .decode(sealed)
        .map_err(|_| INVALID)?
        .try_into()
        .map_err(|_| INVALID)
}

pub(crate) struct StoredDevice {
    pub(crate) reference: String,
    pub(crate) model: String,
    pub(crate) name: String,
    sealed: String,
}

impl StoredDevice {
    pub(crate) fn parse(data: &Value) -> Result<Self, &'static str> {
        let reference = data["device_ref"]
            .as_str()
            .filter(|value| valid_device_ref(value))
            .ok_or(INVALID)?;
        let model = data["model"]
            .as_str()
            .filter(|value| valid_identifier(value))
            .ok_or(INVALID)?;
        let name = data["name"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
            .ok_or(INVALID)?;
        let sealed = data["sealed"].as_str().ok_or(INVALID)?;
        decode(sealed)?;
        Ok(Self {
            reference: reference.to_owned(),
            model: model.to_owned(),
            name: name.to_owned(),
            sealed: sealed.to_owned(),
        })
    }

    pub(crate) fn document(&self) -> Value {
        json!({"device_ref":self.reference,"model":self.model,"name":self.name,"sealed":self.sealed})
    }
}

//! Electrolux JSON commands inside the Broadlink encrypted transport.
use serde_json::Value;

fn checksum(data: &[u8]) -> u16 {
    data.iter()
        .fold(0xc0adu16, |sum, byte| sum.wrapping_add(u16::from(*byte)))
}

pub fn status_request() -> Vec<u8> {
    let mut frame = vec![0x0e, 0, 0xa5, 0xa5, 0x5a, 0x5a, 0, 0, 1, 0x0b, 2, 0, 0, 0];
    frame.extend_from_slice(b"{}");
    let sum = checksum(&frame[8..]);
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    frame
}

pub fn parse_status_response(frame: &[u8]) -> Result<Value, &'static str> {
    if frame.len() < 14 || frame.len() > 8192 {
        return Err("Invalid Electrolux response");
    }
    let expected = u16::from_le_bytes([frame[6], frame[7]]);
    if checksum(&frame[8..]) != expected {
        return Err("Invalid Electrolux checksum");
    }
    let length = u16::from_le_bytes([frame[10], frame[11]]) as usize;
    let payload = frame
        .get(14..14 + length)
        .ok_or("Invalid Electrolux response")?;
    serde_json::from_slice(payload).map_err(|_| "Invalid Electrolux response")
}

pub fn power_request(on: bool) -> Result<Vec<u8>, &'static str> {
    let data: &[u8] = if on {
        br#"{"ac_pwr":1}"#
    } else {
        br#"{"ac_pwr":0}"#
    };
    let mut frame = vec![
        0x18,
        0,
        0xa5,
        0xa5,
        0x5a,
        0x5a,
        0,
        0,
        2,
        0x0b,
        data.len() as u8,
        0,
        0,
        0,
    ];
    frame.extend_from_slice(data);
    let sum = checksum(&frame[8..]);
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    Ok(frame)
}

pub fn temperature_request(temperature: u8) -> Result<Vec<u8>, &'static str> {
    if temperature > 40 {
        return Err("Invalid temperature");
    }
    let data = format!("{{\"temp\":{temperature}}}");
    let mut frame = vec![
        0x17,
        0,
        0xa5,
        0xa5,
        0x5a,
        0x5a,
        0,
        0,
        2,
        0x0b,
        data.len() as u8,
        0,
        0,
        0,
    ];
    frame.extend_from_slice(data.as_bytes());
    let sum = checksum(&frame[8..]);
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    Ok(frame)
}

pub fn mode_request(mode: &str) -> Result<Vec<u8>, &'static str> {
    let code = match mode {
        "cool" => 0,
        "heat" => 1,
        "dry" => 2,
        "fan_only" => 3,
        "auto" => 4,
        "heat_cool" => 6,
        _ => return Err("Invalid mode"),
    };
    let data = format!("{{\"ac_mode\":{code}}}");
    let mut frame = vec![
        0x19,
        0,
        0xa5,
        0xa5,
        0x5a,
        0x5a,
        0,
        0,
        2,
        0x0b,
        data.len() as u8,
        0,
        0,
        0,
    ];
    frame.extend_from_slice(data.as_bytes());
    let sum = checksum(&frame[8..]);
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    Ok(frame)
}

pub fn fan_request(fan: &str) -> Result<Vec<u8>, &'static str> {
    let code = match fan {
        "auto" => 0,
        "low" => 1,
        "medium" => 2,
        "high" => 3,
        "turbo" => 4,
        "quiet" => 5,
        _ => return Err("Invalid fan"),
    };
    let data = format!("{{\"ac_mark\":{code}}}");
    let mut frame = vec![
        0x19,
        0,
        0xa5,
        0xa5,
        0x5a,
        0x5a,
        0,
        0,
        2,
        0x0b,
        data.len() as u8,
        0,
        0,
        0,
    ];
    frame.extend_from_slice(data.as_bytes());
    let sum = checksum(&frame[8..]);
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    Ok(frame)
}

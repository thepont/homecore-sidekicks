use homecore_electrolux::protocol::{parse_status_response, status_request};

#[test]
fn status_request_uses_electrolux_query_frame() {
    assert_eq!(
        status_request(),
        vec![
            0x0e, 0x00, 0xa5, 0xa5, 0x5a, 0x5a, 0xb3, 0xc1, 0x01, 0x0b, 0x02, 0x00, 0x00, 0x00,
            0x7b, 0x7d
        ]
    );
}

#[test]
fn rejects_short_bad_checksum_and_invalid_length_responses() {
    assert!(parse_status_response(&[]).is_err());
    assert!(parse_status_response(&[0; 14]).is_err());
    let mut frame = vec![0; 14];
    frame[10] = 10;
    let sum = frame[8..]
        .iter()
        .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    assert!(parse_status_response(&frame).is_err());
}

#[test]
fn accepts_checksummed_status_payload() {
    let payload = br#"{"envtemp":22}"#;
    let mut frame = vec![0; 14];
    frame[10..12].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    frame.extend_from_slice(payload);
    let sum = frame[8..]
        .iter()
        .fold(0xc0adu16, |s, b| s.wrapping_add(u16::from(*b)));
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    assert_eq!(parse_status_response(&frame).unwrap()["envtemp"], 22);
    frame[14] ^= 1;
    assert!(parse_status_response(&frame).is_err());
}

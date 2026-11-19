use serde_json::json;

#[test]
fn load_power_is_converted_from_watts_to_kilowatts() {
    let status = json!({"response": {"load_power": 1250.0}});
    assert_eq!(homecore_powerwall::teslemetry::load_kw(&status), Ok(1.25));
}

#[test]
fn load_power_is_fetched_from_the_configured_energy_site() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0; 4096];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
        assert!(request.starts_with("get /api/1/energy_sites/42/live_status "));
        assert!(request.contains("authorization: bearer fixture-token\r\n"));
        let body = r#"{"response":{"load_power":1250}}"#;
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert_eq!(
        homecore_powerwall::teslemetry::fetch_load_kw(&client, &base, 42, "fixture-token"),
        Ok(1.25)
    );
    server.join().unwrap();
}

#[test]
fn oversized_upstream_response_is_rejected() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0; 4096];
        let _ = stream.read(&mut buf).unwrap();
        let body = format!(
            "{{\"response\":{{\"load_power\":1250,\"padding\":\"{}\"}}}}",
            "x".repeat(70_000)
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .ok();
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(
        homecore_powerwall::teslemetry::fetch_load_kw(&client, &base, 42, "fixture-token").is_err()
    );
    server.join().unwrap();
}

#[test]
fn powerwall_reading_preserves_charge_percentage_and_signed_power() {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let status = json!({"response": {"timestamp":now.to_rfc3339(),"percentage_charged": 35.5, "battery_power": -1190.0}});
    let reading = homecore_powerwall::teslemetry::powerwall(&status).unwrap();
    assert_eq!(reading.charge_pct, 35.5);
    assert_eq!(reading.power_w, -1190.0);
}

#[test]
fn powerwall_is_read_from_the_energy_site() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        let request = String::from_utf8_lossy(&bytes[..n]).to_ascii_lowercase();
        assert!(request.starts_with("get /api/1/energy_sites/42/live_status "));
        assert!(request.contains("authorization: bearer fixture-token\r\n"));
        let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
        let body = json!({"response":{"timestamp":now.to_rfc3339(),"percentage_charged":65.2,"battery_power":-1190}}).to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert_eq!(
        homecore_powerwall::teslemetry::fetch_powerwall(&client, &base, 42, "fixture-token"),
        Ok(homecore_powerwall::teslemetry::Powerwall {
            charge_pct: 65.2,
            power_w: -1190.0
        })
    );
    server.join().unwrap();
}

#[test]
fn stale_powerwall_reading_is_unavailable() {
    let status = json!({"response": {"timestamp":"2000-01-01T00:00:00Z", "percentage_charged":35.5, "battery_power":-1190}});
    assert!(homecore_powerwall::teslemetry::powerwall(&status).is_err());
}

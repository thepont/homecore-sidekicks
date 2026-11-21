use homecore_hisense::{run, Broker, Hisense};
use std::{
    env,
    fs::File,
    io::Read,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

fn setting(name: &str) -> Result<String, &'static str> {
    env::var(name)
        .ok()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or("Missing or invalid configuration")
}

#[tokio::main]
async fn main() {
    if let Err(error) = start().await {
        eprintln!("Hisense sensor failed: {error}");
        std::process::exit(1);
    }
}

async fn start() -> Result<(), &'static str> {
    let ip: IpAddr = setting("HISENSE_IP")?
        .parse()
        .map_err(|_| "Invalid TV address")?;
    if ip.is_unspecified() || ip.is_multicast() {
        return Err("Invalid TV address");
    }
    let port = setting("HISENSE_PORT")?
        .parse::<u16>()
        .map_err(|_| "Invalid TV port")?;
    let interval = setting("HISENSE_POLL_SECS")?
        .parse::<u64>()
        .map_err(|_| "Invalid poll interval")?;
    if !(5..=300).contains(&interval) {
        return Err("Invalid poll interval");
    }
    let host = setting("MQTT_HOST")?;
    if !matches!(host.as_str(), "127.0.0.1" | "::1") {
        return Err("MQTT must use the local broker");
    }
    let port_mqtt = setting("MQTT_PORT")?
        .parse::<u16>()
        .map_err(|_| "Invalid broker port")?;
    let username = setting("MQTT_USERNAME")?;
    if username.len() > 128 || !username.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Invalid broker user");
    }
    let mut bytes = Vec::new();
    File::open(setting("MQTT_PASSWORD_FILE")?)
        .map_err(|_| "Cannot open broker secret")?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read broker secret")?;
    if bytes.len() > 4096 {
        return Err("Invalid broker secret");
    }
    let password = std::str::from_utf8(&bytes)
        .map_err(|_| "Invalid broker secret")?
        .trim()
        .to_owned();
    if password.is_empty() || !password.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Invalid broker secret");
    }
    let tv = Hisense::new(SocketAddr::new(ip, port), Duration::from_secs(3))?;
    let broker = Broker {
        host,
        port: port_mqtt,
        username,
        password,
    };
    tokio::select! {
        result = run(tv, broker, Duration::from_secs(interval)) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

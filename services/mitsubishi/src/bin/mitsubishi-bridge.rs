use homecore_mitsubishi::{
    bridge::{self, Broker},
    config::{Config, MAX_CONFIG_BYTES},
};
use std::{env, fs::File, io::Read};

fn required(name: &str) -> Result<String, &'static str> {
    env::var(name).map_err(|_| "Missing required environment setting")
}

fn read_bounded(path: &str, limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "Cannot open configuration file")?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read configuration file")?;
    if bytes.len() > limit {
        return Err("Configuration file exceeds size limit");
    }
    Ok(bytes)
}

#[tokio::main]
async fn main() -> Result<(), &'static str> {
    let config = Config::parse(&read_bounded(
        &required("MITSUBISHI_CONFIG_FILE")?,
        MAX_CONFIG_BYTES,
    )?)?;
    let host = required("MQTT_HOST")?;
    let port = required("MQTT_PORT")?
        .parse::<u16>()
        .map_err(|_| "Invalid MQTT port")?;
    // homeCore's deployment keeps the unencrypted broker on the shared loopback network.
    if !matches!(host.as_str(), "127.0.0.1" | "::1") || port == 0 {
        return Err("MQTT must use the local broker");
    }
    let username = required("MQTT_USERNAME")?;
    if username.is_empty()
        || username.len() > 128
        || !username.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err("Invalid MQTT username");
    }
    let bytes = read_bounded(&required("MQTT_PASSWORD_FILE")?, 4096)?;
    let password = std::str::from_utf8(&bytes)
        .map_err(|_| "Invalid MQTT credential")?
        .trim()
        .to_owned();
    if password.is_empty() || !password.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("Invalid MQTT credential");
    }
    let broker = Broker {
        host,
        port,
        username,
        password,
    };
    tokio::select! {
        result = bridge::run(config, broker) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

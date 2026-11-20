use homecore_xiaomi::{login::Provider, runtime, setup::Setup, store::Store};
use std::{env, path::PathBuf};

fn required(name: &str) -> Result<String, &'static str> {
    env::var(name)
        .ok()
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or("Missing or invalid configuration")
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Xiaomi service failed: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let host = required("MQTT_HOST")?;
    let port: u16 = required("MQTT_PORT")?
        .parse()
        .map_err(|_| "Invalid broker port")?;
    if port == 0 {
        return Err("Invalid broker port".into());
    }
    let id = required("MQTT_USERNAME")?;
    let file = required("MQTT_PASSWORD_FILE")?;
    let metadata = std::fs::metadata(&file).map_err(|_| "Cannot read broker secret")?;
    if metadata.len() > 4096 {
        return Err("Invalid broker secret".into());
    }
    let password = std::fs::read_to_string(file)
        .map_err(|_| "Cannot read broker secret")?
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    if password.is_empty() {
        return Err("Invalid broker secret".into());
    }
    let root = PathBuf::from(required("XIAOMI_STATE_DIR")?);
    let ui = PathBuf::from(required("XIAOMI_UI_DIR")?);
    let region = env::var("XIAOMI_REGION").unwrap_or_else(|_| "cn".to_owned());
    if !matches!(region.as_str(), "cn" | "de" | "i2" | "ru" | "sg" | "us") {
        return Err("Invalid Xiaomi region".into());
    }
    let setup = Setup::new(
        Provider::new("https://account.xiaomi.com/")?,
        Store::new(&root),
        &region,
    );
    runtime::run(host, port, id, password, setup, ui).await
}

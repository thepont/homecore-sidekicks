use homecore_xiaomi::{extractor, login::Provider};
use std::env;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if run().await.is_err() {
        eprintln!("Xiaomi extractor could not start");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let secret = env::var_os("XIAOMI_EXTRACTOR_AUTH_FILE").ok_or("Missing service key")?;
    let key = extractor::read_service_key(std::path::Path::new(&secret))?;
    let port = env::var("PORT")
        .as_deref()
        .unwrap_or("8080")
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).await?;
    axum::serve(
        listener,
        extractor::router(Provider::new("https://account.xiaomi.com/")?, key),
    )
    .await?;
    Ok(())
}

use crate::{login::Provider, store::Store};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Setup {
    provider: Arc<Provider>,
    store: Arc<Store>,
    region: String,
    state: Arc<Mutex<&'static str>>,
}
impl Setup {
    pub fn new(provider: Provider, store: Store, region: &str) -> Self {
        let state = match store.load_gateway() {
            Ok(Some(_)) => "gateway_saved",
            Ok(None) => "unconfigured",
            Err(_) => "error",
        };
        Self {
            provider: Arc::new(provider),
            store: Arc::new(store),
            region: region.to_owned(),
            state: Arc::new(Mutex::new(state)),
        }
    }
    pub async fn status(&self) -> &'static str {
        *self.state.lock().await
    }
    pub async fn begin(&self) -> Result<String, &'static str> {
        let mut state = self.state.lock().await;
        if *state == "waiting" {
            return Err("Sign-in already in progress");
        }
        let challenge = self.provider.begin().await?;
        let qr = challenge.qr_png.clone();
        *state = "waiting";
        let setup = self.clone();
        tokio::spawn(async move {
            let expiry = challenge.expires_after;
            let result = tokio::time::timeout(expiry, async {
                let auth = setup.provider.poll(&challenge).await?;
                let gateways = setup.provider.discover(&auth, &setup.region).await?;
                if gateways.len() != 1 {
                    return Err("Expected one gateway");
                }
                gateways[0].save_configuration(&setup.store)
            })
            .await;
            *setup.state.lock().await = match result {
                Ok(Ok(())) => "gateway_saved",
                _ => "error",
            };
        });
        Ok(qr)
    }
}

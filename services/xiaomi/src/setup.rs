use crate::{adapters::MatchedDevice, login::Provider, store::Store};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Setup {
    provider: Arc<Provider>,
    store: Arc<Store>,
    region: String,
    state: Arc<Mutex<SetupState>>,
}
struct SetupState {
    status: &'static str,
    qr_png: Option<String>,
    devices: Option<Vec<MatchedDevice>>,
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
            state: Arc::new(Mutex::new(SetupState {
                status: state,
                qr_png: None,
                devices: None,
            })),
        }
    }
    pub async fn status(&self) -> &'static str {
        self.state.lock().await.status
    }
    pub async fn snapshot(&self) -> (&'static str, Option<String>) {
        let state = self.state.lock().await;
        (state.status, state.qr_png.clone())
    }
    pub(crate) async fn action_status(&self) -> serde_json::Value {
        let state = self.state.lock().await;
        let mut result = serde_json::json!({
            "status": "ok", "setup_status": state.status, "qr_png": state.qr_png
        });
        if let Some(devices) = &state.devices {
            result["devices"] =
                serde_json::Value::Array(devices.iter().map(MatchedDevice::to_json).collect());
        }
        result
    }
    pub async fn begin(&self) -> Result<String, &'static str> {
        self.begin_with_region(&self.region).await
    }
    pub async fn begin_with_region(&self, region: &str) -> Result<String, &'static str> {
        if !matches!(region, "cn" | "de" | "i2" | "ru" | "sg" | "us") {
            return Err("Invalid Xiaomi region");
        }
        let mut state = self.state.lock().await;
        if state.status == "waiting" {
            return Err("Sign-in already in progress");
        }
        let adapters = self.store.adapters()?;
        let challenge = self.provider.begin().await?;
        let qr = challenge.qr_png.clone();
        state.status = "waiting";
        state.qr_png = Some(qr.clone());
        state.devices = None;
        let mut setup = self.clone();
        setup.region = region.to_owned();
        tokio::spawn(async move {
            let expiry = challenge.expires_after;
            let result = tokio::time::timeout(expiry, async {
                let auth = setup.provider.poll(&challenge).await?;
                if let Some(catalog) = &adapters {
                    return setup
                        .provider
                        .discover_registered(&auth, &setup.region, catalog)
                        .await
                        .map(Some);
                }
                let gateways = setup.provider.discover(&auth, &setup.region).await?;
                if gateways.len() != 1 {
                    return Err("Expected one gateway");
                }
                gateways[0].save_configuration(&setup.store)?;
                Ok(None)
            })
            .await;
            let mut state = setup.state.lock().await;
            state.status = match result {
                Ok(Ok(Some(devices))) => {
                    state.devices = Some(devices);
                    "devices_found"
                }
                Ok(Ok(None)) => "gateway_saved",
                _ => "error",
            };
            state.qr_png = None;
        });
        Ok(qr)
    }
}

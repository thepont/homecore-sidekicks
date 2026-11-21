use crate::{
    adapters::{AdapterCatalog, MatchedDevice},
    login::Provider,
    registry,
    store::Store,
};
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Setup {
    provider: Arc<Provider>,
    store: Arc<Store>,
    region: String,
    state: Arc<Mutex<SetupState>>,
    adapters: AdapterSource,
}

#[derive(Clone)]
enum AdapterSource {
    File,
    Registry(Option<Arc<AdapterCatalog>>),
}
enum SetupState {
    Unconfigured,
    Waiting { qr_png: String },
    GatewaySaved,
    DevicesFound(Vec<MatchedDevice>),
    Error,
}

impl SetupState {
    fn status(&self) -> &'static str {
        match self {
            Self::Unconfigured => "unconfigured",
            Self::Waiting { .. } => "waiting",
            Self::GatewaySaved => "gateway_saved",
            Self::DevicesFound(_) => "devices_found",
            Self::Error => "error",
        }
    }

    fn qr_png(&self) -> Option<&str> {
        match self {
            Self::Waiting { qr_png } => Some(qr_png),
            _ => None,
        }
    }
}

impl Setup {
    pub fn new(provider: Provider, store: Store, region: &str) -> Self {
        let state = match store.load_gateway() {
            Ok(Some(_)) => SetupState::GatewaySaved,
            Ok(None) => SetupState::Unconfigured,
            Err(_) => SetupState::Error,
        };
        Self {
            provider: Arc::new(provider),
            store: Arc::new(store),
            region: region.to_owned(),
            state: Arc::new(Mutex::new(state)),
            adapters: AdapterSource::File,
        }
    }

    pub async fn with_sqlite(mut self, database: &Path) -> Result<Self, &'static str> {
        let catalog = registry::load_sqlite(database.to_path_buf(), self.store.adapters()?).await?;
        self.adapters = AdapterSource::Registry(catalog.map(Arc::new));
        Ok(self)
    }

    #[cfg(feature = "postgres")]
    pub async fn with_postgres(
        mut self,
        dsn_file: &Path,
        ca_file: Option<&Path>,
    ) -> Result<Self, &'static str> {
        let catalog = registry::load_postgres(dsn_file, ca_file, self.store.adapters()?).await?;
        self.adapters = AdapterSource::Registry(catalog.map(Arc::new));
        Ok(self)
    }
    pub async fn status(&self) -> &'static str {
        self.state.lock().await.status()
    }
    pub async fn snapshot(&self) -> (&'static str, Option<String>) {
        let state = self.state.lock().await;
        (state.status(), state.qr_png().map(str::to_owned))
    }
    pub(crate) async fn action_status(&self) -> serde_json::Value {
        let state = self.state.lock().await;
        let mut result = serde_json::json!({
            "status": "ok", "setup_status": state.status(), "qr_png": state.qr_png()
        });
        if let SetupState::DevicesFound(devices) = &*state {
            result["devices"] =
                serde_json::Value::Array(devices.iter().map(MatchedDevice::to_json).collect());
        }
        if let AdapterSource::Registry(Some(catalog)) = &self.adapters {
            result["registered_adapters"] = catalog.registrations();
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
        if matches!(*state, SetupState::Waiting { .. }) {
            return Err("Sign-in already in progress");
        }
        let adapters = match &self.adapters {
            AdapterSource::File => self.store.adapters()?.map(Arc::new),
            AdapterSource::Registry(catalog) => catalog.clone(),
        };
        let challenge = self.provider.begin().await?;
        let qr = challenge.qr_png.clone();
        *state = SetupState::Waiting { qr_png: qr.clone() };
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
            *setup.state.lock().await = match result {
                Ok(Ok(Some(devices))) => SetupState::DevicesFound(devices),
                Ok(Ok(None)) => SetupState::GatewaySaved,
                _ => SetupState::Error,
            };
        });
        Ok(qr)
    }
}

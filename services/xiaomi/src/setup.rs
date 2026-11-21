use crate::{
    adapters::{AdapterCatalog, MatchedDevice},
    extractor_client::{Extraction, ExtractorClient},
    login::{Challenge, DiscoveredDevice, Provider},
    registry::{Operation, Registry, RegistryState},
    store::Store,
    transport::valid_region,
    vault::Vault,
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
    vault: Option<Arc<Vault>>,
    extractor: Option<Arc<ExtractorClient>>,
}

enum SignIn {
    Local(Challenge),
    Remote(Extraction),
}

impl SignIn {
    fn qr(&self) -> &str {
        match self {
            Self::Local(challenge) => &challenge.qr_png,
            Self::Remote(extraction) => &extraction.qr_png,
        }
    }
}

#[derive(Clone)]
enum AdapterSource {
    File,
    Registry(Arc<AdapterRegistry>),
}

struct AdapterRegistry {
    repository: Registry,
    state: Mutex<RegistryState>,
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
            vault: None,
            extractor: None,
        }
    }

    async fn with_registry(mut self, repository: Registry) -> Result<Self, &'static str> {
        let state = repository
            .run(Operation::Bootstrap(self.store.adapters()?))
            .await?;
        self.adapters = AdapterSource::Registry(Arc::new(AdapterRegistry {
            repository,
            state: Mutex::new(state),
        }));
        Ok(self)
    }

    pub async fn with_sqlite(self, database: &Path) -> Result<Self, &'static str> {
        self.with_registry(Registry::Sqlite(database.to_path_buf()))
            .await
    }

    pub fn with_vault_key_file(mut self, key: &Path) -> Result<Self, &'static str> {
        if !matches!(self.adapters, AdapterSource::Registry(_)) {
            return Err("Xiaomi credential storage requires a registry");
        }
        self.vault = Some(Arc::new(Vault::from_file(key)?));
        Ok(self)
    }

    pub(crate) fn vault(&self) -> Result<Arc<Vault>, &'static str> {
        self.vault
            .clone()
            .ok_or("Xiaomi credential storage is not configured")
    }

    pub fn with_extractor(
        mut self,
        origin: &str,
        key_file: &Path,
        ca_file: Option<&Path>,
    ) -> Result<Self, &'static str> {
        self.extractor = Some(Arc::new(ExtractorClient::new(origin, key_file, ca_file)?));
        Ok(self)
    }

    #[cfg(feature = "postgres")]
    pub async fn with_postgres(
        self,
        dsn_file: &Path,
        ca_file: Option<&Path>,
    ) -> Result<Self, &'static str> {
        self.with_registry(Registry::postgres(dsn_file, ca_file)?)
            .await
    }

    pub(crate) async fn update_registry(&self, action: Operation) -> Result<(), &'static str> {
        let AdapterSource::Registry(registry) = &self.adapters else {
            return Err("Xiaomi access requests require a registry");
        };
        let mut state = registry.state.lock().await;
        *state = registry.repository.run(action).await?;
        Ok(())
    }

    pub(crate) async fn has_pending_requests(&self) -> bool {
        match &self.adapters {
            AdapterSource::File => false,
            AdapterSource::Registry(registry) => registry.state.lock().await.has_pending_requests(),
        }
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
        drop(state);
        if let AdapterSource::Registry(registry) = &self.adapters {
            let registry = registry.state.lock().await;
            if let Some(catalog) = &registry.catalog {
                result["registered_adapters"] = catalog.registrations();
            }
            result["access_requests"] = registry.pending_requests();
            if self.vault.is_some() {
                result["known_devices"] = registry.known_devices();
            }
        }
        result
    }
    pub async fn begin(&self) -> Result<String, &'static str> {
        self.begin_with_region(&self.region).await
    }
    pub async fn begin_with_region(&self, region: &str) -> Result<String, &'static str> {
        if !valid_region(region) {
            return Err("Invalid Xiaomi region");
        }
        let mut state = self.state.lock().await;
        if matches!(*state, SetupState::Waiting { .. }) {
            return Err("Sign-in already in progress");
        }
        let adapters = match &self.adapters {
            AdapterSource::File => self.store.adapters()?,
            AdapterSource::Registry(registry) => registry.state.lock().await.catalog.clone(),
        };
        let sign_in = match &self.extractor {
            Some(extractor) => {
                let catalog = adapters.as_ref().ok_or("Register Xiaomi adapters first")?;
                SignIn::Remote(extractor.begin(region, catalog).await?)
            }
            None => SignIn::Local(self.provider.begin().await?),
        };
        let qr = sign_in.qr().to_owned();
        *state = SetupState::Waiting { qr_png: qr.clone() };
        let setup = self.clone();
        let region = region.to_owned();
        tokio::spawn(async move {
            let result = setup
                .complete_sign_in(sign_in, &region, adapters.as_ref())
                .await;
            *setup.state.lock().await = match result {
                Ok(Some(devices)) => SetupState::DevicesFound(devices),
                Ok(None) => SetupState::GatewaySaved,
                _ => SetupState::Error,
            };
        });
        Ok(qr)
    }

    async fn complete_sign_in(
        &self,
        sign_in: SignIn,
        region: &str,
        catalog: Option<&AdapterCatalog>,
    ) -> Result<Option<Vec<MatchedDevice>>, &'static str> {
        let capture = self.vault.is_some();
        let discovered = match sign_in {
            SignIn::Local(challenge) => tokio::time::timeout(challenge.expires_after, async {
                let auth = self.provider.poll(&challenge).await?;
                if let Some(catalog) = catalog {
                    return self
                        .provider
                        .discover_registered(&auth, region, catalog, capture)
                        .await
                        .map(Some);
                }
                let gateways = self.provider.discover(&auth, region).await?;
                if gateways.len() != 1 {
                    return Err("Expected one gateway");
                }
                gateways[0].save_configuration(&self.store)?;
                Ok(None)
            })
            .await
            .map_err(|_| "Xiaomi sign-in timed out")??,
            SignIn::Remote(extraction) => {
                let catalog = catalog.ok_or("Register Xiaomi adapters first")?;
                Some(extraction.finish(catalog, capture).await?)
            }
        };
        let Some(discovered) = discovered else {
            return Ok(None);
        };
        self.remember_devices(region, &discovered).await?;
        Ok(Some(
            discovered.into_iter().map(|record| record.device).collect(),
        ))
    }

    async fn remember_devices(
        &self,
        region: &str,
        devices: &[DiscoveredDevice],
    ) -> Result<(), &'static str> {
        if let Some(vault) = &self.vault {
            let encrypted = devices
                .iter()
                .map(|device| vault.seal(region, device))
                .collect::<Result<_, _>>()?;
            self.update_registry(Operation::Remember(encrypted)).await?;
        }
        Ok(())
    }
}

//! Configure reqwest with the ring provider and bundled public trust roots.

use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;

pub fn client_config(roots: Option<RootCertStore>) -> ClientConfig {
    let mut roots = roots.unwrap_or_else(RootCertStore::empty);
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}

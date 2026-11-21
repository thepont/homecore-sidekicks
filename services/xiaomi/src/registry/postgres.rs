use super::{parse_document, AdapterCatalog, DATABASE_ERROR, SAVE, SCHEMA, SELECT};
use rustls::{
    pki_types::{pem::PemObject, CertificateDer},
    ClientConfig, RootCertStore,
};
use std::{
    fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, path::Path, sync::Arc, time::Duration,
};
use tokio_postgres::{config::SslMode, Config};
use tokio_postgres_rustls::MakeRustlsConnect;

pub(crate) async fn load_postgres(
    dsn_path: &Path,
    ca_path: Option<&Path>,
    import: Option<AdapterCatalog>,
) -> Result<Option<AdapterCatalog>, &'static str> {
    let mut config: Config = read_file(dsn_path, 4096)?
        .trim()
        .parse()
        .map_err(|_| DATABASE_ERROR)?;
    config.ssl_mode(SslMode::Require);
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(path) = ca_path {
        let pem = read_file(path, 65536)?;
        let certificates: Vec<_> = CertificateDer::pem_slice_iter(pem.as_bytes())
            .collect::<Result<_, _>>()
            .map_err(|_| DATABASE_ERROR)?;
        if certificates.is_empty() {
            return Err(DATABASE_ERROR);
        }
        for certificate in certificates {
            roots.add(certificate).map_err(|_| DATABASE_ERROR)?;
        }
    }
    let tls =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|_| DATABASE_ERROR)?
            .with_root_certificates(roots)
            .with_no_client_auth();
    tokio::time::timeout(Duration::from_secs(6), async {
        let (mut client, connection) = config
            .connect(MakeRustlsConnect::new(tls))
            .await
            .map_err(|_| DATABASE_ERROR)?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let transaction = client.transaction().await.map_err(|_| DATABASE_ERROR)?;
        // Serialize first-time schema creation by replicas of this service.
        transaction
            .batch_execute("SELECT pg_advisory_xact_lock(1481197647, 1)")
            .await
            .map_err(|_| DATABASE_ERROR)?;
        transaction
            .batch_execute(SCHEMA)
            .await
            .map_err(|_| DATABASE_ERROR)?;
        let catalog = if let Some(catalog) = import {
            let document = catalog.registrations().to_string();
            transaction
                .execute(SAVE, &[&document])
                .await
                .map_err(|_| DATABASE_ERROR)?;
            Some(catalog)
        } else {
            let row = transaction
                .query_opt(SELECT, &[])
                .await
                .map_err(|_| DATABASE_ERROR)?;
            let document = row
                .as_ref()
                .map(|row| row.try_get::<_, &str>(0))
                .transpose()
                .map_err(|_| DATABASE_ERROR)?;
            parse_document(document)?
        };
        transaction.commit().await.map_err(|_| DATABASE_ERROR)?;
        Ok(catalog)
    })
    .await
    .map_err(|_| DATABASE_ERROR)?
}

fn read_file(path: &Path, max_bytes: u64) -> Result<String, &'static str> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| DATABASE_ERROR)?;
    if !file.metadata().map_err(|_| DATABASE_ERROR)?.is_file() {
        return Err(DATABASE_ERROR);
    }
    let mut data = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut data)
        .map_err(|_| DATABASE_ERROR)?;
    if data.is_empty() || data.len() as u64 > max_bytes {
        return Err(DATABASE_ERROR);
    }
    Ok(data)
}

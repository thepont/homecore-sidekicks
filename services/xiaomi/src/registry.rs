use crate::adapters::AdapterCatalog;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::{
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres")]
pub(crate) use postgres::load_postgres;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS xiaomi_adapter_registry (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        registrations TEXT NOT NULL CHECK (length(registrations) <= 65536)
    )";
const SELECT: &str = "SELECT registrations FROM xiaomi_adapter_registry WHERE id = 1";
const SAVE: &str = "
    INSERT INTO xiaomi_adapter_registry (id, registrations) VALUES (1, $1)
    ON CONFLICT (id) DO UPDATE SET registrations = excluded.registrations
    WHERE xiaomi_adapter_registry.registrations <> excluded.registrations";
const DATABASE_ERROR: &str = "Cannot load the Xiaomi adapter database";

pub(crate) async fn load_sqlite(
    path: PathBuf,
    import: Option<AdapterCatalog>,
) -> Result<Option<AdapterCatalog>, &'static str> {
    tokio::task::spawn_blocking(move || {
        let connection = open_sqlite(&path)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|_| DATABASE_ERROR)?;
        if let Some(catalog) = import {
            let document = catalog.registrations().to_string();
            connection
                .execute(SAVE, [document])
                .map_err(|_| DATABASE_ERROR)?;
            return Ok(Some(catalog));
        }
        let document: Option<String> = connection
            .query_row(SELECT, [], |row| row.get(0))
            .optional()
            .map_err(|_| DATABASE_ERROR)?;
        parse_document(document.as_deref())
    })
    .await
    .map_err(|_| DATABASE_ERROR)?
}

fn open_sqlite(path: &Path) -> Result<Connection, &'static str> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| DATABASE_ERROR)?;
    let metadata = file.metadata().map_err(|_| DATABASE_ERROR)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(DATABASE_ERROR);
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| DATABASE_ERROR)?;
    connection
        .busy_timeout(Duration::from_secs(3))
        .map_err(|_| DATABASE_ERROR)?;
    Ok(connection)
}

fn parse_document(document: Option<&str>) -> Result<Option<AdapterCatalog>, &'static str> {
    document
        .map(|document| {
            if document.len() > 65536 {
                return Err(DATABASE_ERROR);
            }
            let data = serde_json::from_str(document).map_err(|_| DATABASE_ERROR)?;
            AdapterCatalog::parse(&data)
        })
        .transpose()
}

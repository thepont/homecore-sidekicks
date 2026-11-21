# Shared Xiaomi setup

The Xiaomi plugin provides one admin setup page under **Settings → Plugins**.
Select the Xiaomi Home region and scan the sign-in QR code. When adapter
registrations are present, the page shows compatible LAN devices and the
registrations that match each model.

## Adapter registrations

The deployment can supply a read-only `adapters.json` in `XIAOMI_STATE_DIR`.
Each entry declares an installed adapter's HomeCore plugin ID and the exact
Xiaomi model IDs it supports. The setup plugin imports the file at startup, so
an adapter can be registered before its process starts. These entries show
the format using example plugin IDs.

```json
[
  {"plugin_id": "plugin.xiaomi-gateway", "models": ["lumi.gateway.mcn001"]},
  {"plugin_id": "plugin.xiaomi-vacuum", "models": ["ijai.vacuum.v10"]}
]
```

Only deployment or installation tooling should write this file. Restart the
setup plugin after updating the file. Duplicate plugin
IDs and repeated models in one registration are rejected. Multiple plugins
can support the same model, in which case the setup page lists all of them.
An empty array enables shared setup with no registered models.

The default `XIAOMI_DATABASE=sqlite` stores the registration list in
`xiaomi.sqlite3` in the persistent state directory. The setup plugin reads
the saved list at startup when `adapters.json` is absent. When neither a
saved list nor an import file is present, setup uses the single-gateway flow.
The SQLite connection closes once the catalogue is loaded.

Build with `cargo build --locked --release --features postgres` to enable
the PostgreSQL driver. Set `XIAOMI_DATABASE=postgres` and
`XIAOMI_POSTGRES_URL_FILE` to a mounted secret containing the connection URL.
The database role needs permission to create and update its registration
table. The connection requires TLS and verifies the
server certificate. Set `XIAOMI_POSTGRES_CA_FILE` for a private CA. Like
SQLite, PostgreSQL is used during startup and the setup service serves the
loaded catalogue from memory.

`Setup` is the Facade for QR sign-in and discovery. The adapter catalogue
owns model matching, and the Xiaomi provider fetches the account's devices
once per sign-in. The setup response contains device IDs, names, models and
matching plugin IDs. Credentials stay in the provider during matching.

This first slice covers discovery and matching. The catalogue is supplied by
the deployment today. Registering it automatically at plugin install,
assigning credentials, starting adapters and moving extraction to a Knative
worker are follow-up slices.

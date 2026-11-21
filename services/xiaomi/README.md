# Shared Xiaomi setup

The Xiaomi plugin provides one admin setup page under **Settings → Plugins**.
Select the Xiaomi Home region and scan the sign-in QR code. When adapter
registrations are present, the page shows compatible LAN devices and the
registrations that match each model.

## Adapter registrations

The deployment can supply a read-only `adapters.json` in `XIAOMI_STATE_DIR`.
Each entry declares an installed adapter's HomeCore plugin ID and the exact
Xiaomi model IDs it supports. The setup plugin seeds a new registry at startup, so
an adapter can be registered before its process starts. These entries show
the format using example plugin IDs.

```json
[
  {"plugin_id": "plugin.xiaomi-gateway", "models": ["lumi.gateway.mcn001"]},
  {"plugin_id": "plugin.xiaomi-vacuum", "models": ["ijai.vacuum.v10"]}
]
```

Only deployment or installation tooling should write this file. Update
existing registrations through HomeCore. Duplicate plugin IDs and repeated
models in one registration are rejected. Multiple plugins
can support the same model, in which case the setup page lists all of them.
An empty array enables shared setup with no registered models.

The default `XIAOMI_DATABASE=sqlite` stores the registration list in
`xiaomi.sqlite3` in the persistent state directory. The setup plugin prefers
the saved list at startup. When neither a
saved list nor an import file is present, setup uses the single-gateway flow.
The SQLite connection closes once the catalogue is loaded.

Build with `cargo build --locked --release --features postgres` to enable
the PostgreSQL driver. Set `XIAOMI_DATABASE=postgres` and
`XIAOMI_POSTGRES_URL_FILE` to a mounted secret containing the connection URL.
The database role needs permission to create and update its registration
table. The connection requires TLS and verifies the
server certificate. Set `XIAOMI_POSTGRES_CA_FILE` for a private CA.

## Request access on behalf of an adapter

Installation tooling with a HomeCore admin identity can POST the following
action bodies to `/api/v1/plugins/plugin.xiaomi/command`.

```json
{"action":"register_adapter","plugin_id":"plugin.xiaomi-vacuum","models":["ijai.vacuum.v10"]}
```

```json
{"action":"request_access","plugin_id":"plugin.xiaomi-vacuum","model":"ijai.vacuum.v10"}
```

The request queues one approval per adapter and model. The Xiaomi setup page
shows pending requests. Reads come from memory. The repository opens a
database connection for startup and writes, then closes it.

## Save credentials and approve access

Set `XIAOMI_VAULT_KEY_FILE` to a mounted deployment secret containing
exactly 32 random bytes. Keep the key with the deployment secrets and
maintain a separate backup of it. Both databases store the LAN address and
token encrypted with AES-256-GCM. The setup page receives device names,
models, matching adapters and grants.

Sign in to Xiaomi once to save matching devices. Under **Xiaomi access
requests**, choose the saved device to grant to the requesting adapter.
Repeat approvals use the saved encrypted device record. Deny a pending
request or revoke access for an adapter from the same page.

`Setup` is the Facade for QR sign-in, discovery and access requests.
Typed Commands dispatch HomeCore actions. The Registry is the Repository
for adapter registrations and requests. The adapter catalogue
owns model matching, and the Xiaomi provider fetches the account's devices
once per sign-in. The setup response contains device IDs, names, models and
matching plugin IDs. Credentials stay in the provider during matching.

## Deploy an on-demand extractor

Build the worker without the resident setup plugin's database and MQTT
drivers.

```sh
cargo zigbuild --locked --release --target x86_64-unknown-linux-musl \
  --no-default-features --features extractor --bin xiaomi-extractor
```

Place the resulting static binary in the build context for
[`Dockerfile.xiaomi-extractor`](../../deploy/Dockerfile.xiaomi-extractor).
The [Knative example](../../deploy/xiaomi-extractor.knative.example.yaml)
uses zero minimum replicas and one concurrent sign-in per replica. Enable
cluster-local HTTPS on the route and restrict it to the setup service.

Mount the same 32-byte random extractor key in the setup plugin and worker
as `XIAOMI_EXTRACTOR_AUTH_FILE`. The extractor key is a separate secret from
the vault key. Set `XIAOMI_EXTRACTOR_URL` on the setup plugin to the
extractor's HTTPS origin. Set `XIAOMI_EXTRACTOR_CA_FILE` if the route uses
a private CA.

The setup plugin opens one HTTP response for the duration of a sign-in.
The worker sends the QR first and sends matching device data when Xiaomi
approves the session. The open response keeps the worker active. Pending
requests, approval and status use the resident plugin while the worker is
at zero replicas. Local QR sign-in remains available when no extractor
URL is configured.

Credential delivery to adapter processes and automatic adapter startup are
follow-up slices.

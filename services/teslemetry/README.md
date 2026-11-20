# Teslemetry adapter for Powerwall and vehicles

`teslemetry-bridge` reads a Teslemetry account and publishes its readings to
homeCore over MQTT. It runs continuously as the `teslemetry` container in the
trial's core pod and reads site configuration, daily energy totals, Wall
Connectors and discovered vehicles.

Use it when you want Tesla readings in homeCore without adding battery or
vehicle controls. The adapter makes read requests to Teslemetry, never wakes a
car, and exposes no HTTP server of its own. The [deployment guide](../../deploy/README.md)
covers the running trial and its private credentials.

## Build and test

Run these commands from `services/teslemetry`:

```sh
cargo test --locked
cargo build --release --locked --bin teslemetry-bridge
```

The default suite checks parsing, validation and source requests using local
fixtures. It also checks that an oversized credential is rejected without
waiting indefinitely for the file reader. It does not contact the real account.

The MQTT contract tests start the actual adapter against a fake Teslemetry
server and an authenticated Mosquitto broker. They cover registration, sensor
schemas, unavailable readings, totals and vehicle telemetry. Docker must be
running, and the pinned broker image must be available:

```sh
docker pull eclipse-mosquitto@sha256:6a332cc7a8dbe011cbc076180cbb76b37c6f0f55809682ea08e7980ef0112ad9
cargo test --locked --test teslemetry_adapter -- --ignored
```

Each fixture owns its temporary broker and publishes it on a loopback port.
These tests do not need household credentials or a running homeCore instance.

Two optional tests read the real account. Set paths to private files outside
this repository before running them:

```sh
export TESLEMETRY_TOKEN_FILE=/absolute/private/path/token
export TESLEMETRY_SITE_ID_FILE=/absolute/private/path/site-id
cargo test --locked --test powerwall_live -- --ignored
cargo test --locked --test energy_stream_live -- --ignored
```

The first compares successive live readings, allowing for changes between
samples. The second needs Docker and checks that live power, streamed solar
energy and the site reserve reach a temporary broker within 15 seconds. It can
fail if the account does not supply those streams in time. These tests read
Teslemetry; they do not publish into the household homeCore broker. Vehicle
behaviour has been tested with a fake source, not a real car on this account.

## Configure a run

The service directory, executable and container use the name `teslemetry`. The
existing `homecore-powerwall` secret and `plugin.teslemetry-powerwall` MQTT
identity keep their names so a rename does not create a second set of devices.

The binary takes its configuration from environment variables. Credentials are
read from files, so their values need not appear in command arguments.

| Variable | Value |
| --- | --- |
| `TESLEMETRY_URL` | API origin, normally `https://api.teslemetry.com`. Use HTTPS; HTTP is accepted only for a fixture at `127.0.0.1`. Paths other than `/`, URL credentials, query strings and fragments are rejected. |
| `TESLEMETRY_SITE_ID` | Positive numeric energy-site ID. Supply the number itself, without surrounding whitespace. |
| `TESLEMETRY_TOKEN_FILE` | Path to the Teslemetry bearer-token file. |
| `MQTT_HOST` | Must be exactly `127.0.0.1`. The broker must share the adapter's network namespace. |
| `MQTT_PORT` | Broker port, from `1` to `65535`; the trial uses `1883`. |
| `MQTT_USERNAME` | Broker account; the trial uses `plugin.teslemetry-powerwall`. |
| `MQTT_PASSWORD_FILE` | Path to the password file for that broker account. |
| `POLL_INTERVAL_SECS` | Optional delay between completed live polls. Defaults to `60`; accepts `1` through `3600`. |

All variables except the poll interval are required. Each secret file must be
at most 4,096 bytes. The reader trims surrounding whitespace and then requires
a nonempty value made of printable ASCII characters without spaces. File paths
must be readable by the process user. The binary reads these files at startup;
restart it after replacing a credential.

The live tests use `TESLEMETRY_SITE_ID_FILE`, but the binary uses
`TESLEMETRY_SITE_ID`. They are different interfaces: giving the binary a path
where it expects a number will fail.

For a manual run, first arrange a local broker account and the two credential
files. Use a separate development broker if the deployed adapter is already
running: the Powerwall MQTT client ID is fixed, so a second copy on the same
broker can disconnect the first. From `services/teslemetry`, replace the sample
site ID and file paths below with your own:

```sh
export TESLEMETRY_URL=https://api.teslemetry.com
export TESLEMETRY_SITE_ID=42
export TESLEMETRY_TOKEN_FILE=/absolute/private/path/token
export MQTT_HOST=127.0.0.1
export MQTT_PORT=1883
export MQTT_USERNAME=plugin.teslemetry-powerwall
export MQTT_PASSWORD_FILE=/absolute/private/path/mqtt-password
export POLL_INTERVAL_SECS=60
cargo run --release --locked --bin teslemetry-bridge
```

Stop a manual run with Ctrl-C. In Kubernetes, the manifest supplies these values
and mounts the secret files. Its broker account must match the private
`core/config/plugins/plugin.teslemetry-powerwall.toml` configuration. The broker
has no topic ACLs, so this account and the containers in the pod share a trust
boundary. The adapter does not provide a remote MQTT or MQTT-over-TLS mode.

## How it works

There are three concurrent jobs. The binary starts them after reading its
configuration and setting up the MQTT connection.

1. **Poll the energy site.** Request
   `/api/1/energy_sites/{site}/live_status`, check the reading, and publish the
   sensor schema, state and availability. Sleep for the configured interval
   after each attempt. A slow request therefore lengthens the time between
   updates; the interval is not an exact wall-clock schedule.
2. **Listen for site events.** Keep a server-sent events connection open to
   `/sse?topics=energy_totals,wall_connector_totals,site_info`. Validate events
   for the configured site and hold the latest accepted fields in memory.
   Merge them into the next successful live publication.
3. **Discover and follow vehicles.** Read `/api/1/vehicles` at startup and then
   wait 15 minutes between discovery attempts. For each valid discovered VIN,
   listen at `/sse/{vin}?topics=state,data,vehicle_data` and publish a separate
   homeCore sensor device.

Ordinary HTTP requests have a ten-second timeout. Stream connections have a
ten-second connection timeout and a 120-second read timeout; failed streams
retry after two seconds. HTTP redirects are disabled. The input readers bound
response and event buffers to keep an oversized source message from growing
without limit.

### Powerwall readings

A live reading needs a valid timestamp, a charge percentage between 0 and 100,
and finite battery power. The timestamp may be at most five minutes old or two
minutes ahead of the adapter's clock. An invalid optional field can also reject
the whole reading; missing optional fields are simply absent.

| homeCore field | Meaning |
| --- | --- |
| `charge_pct` | Battery charge, in percent. |
| `battery_power_w` | Battery power, with the sign supplied by Teslemetry. |
| `solar_power_w`, `home_power_w`, `grid_power_w`, `generator_power_w` | Available live power readings, in watts. Signs are preserved. |
| `grid_status`, `island_status`, `storm_mode_active` | Reported grid/island state and active Storm Watch status. |
| `wall_connector_1_power_w` and other numbered connector fields | Readings indexed by their position in the source array, starting at 1. State and fault values remain raw numeric codes. |
| `total_home_usage_wh`, `total_solar_generation_wh` and other `*_wh` totals | Daily energy from the totals stream, in watt-hours. |
| `energy_totals_date` | Source date for the daily energy totals. |
| `wall_connectors_energy_wh`, `wall_connectors_energy_date` | Sum of the reported connector energy totals and their source date. |
| `site_backup_reserve_pct`, `site_operation_mode` and other `site_*` fields | Site configuration reported by the stream; these are readings, not controls. |

The card's primary fields are charge and battery power. Every published
attribute is read-only. Connector serial numbers are omitted, and connector
numbers should not be treated as permanent identities. The aggregate charger
energy is hidden when the live reading has no connector fields.

Energy totals accept dates within one day of the current UTC date to allow for
local date differences. Each stream topic tracks its newest accepted
`createdAt` timestamp so an older replay cannot replace newer data. The cache
merges reported fields; it does not expire totals when the stream stops. A
successful live poll can therefore include an older total. Read its source
date, and use live power for a current reading. Restarting the process clears
the in-memory stream cache.

### MQTT messages and availability

The adapter registers the Powerwall as `teslemetry_powerwall` under plugin
`plugin.teslemetry-powerwall`. These are its main topics:

| Topic | Payload |
| --- | --- |
| `homecore/plugins/plugin.teslemetry-powerwall/register` | Device identity, display name and sensor type. |
| `homecore/devices/teslemetry_powerwall/schema` | Attribute types, units, read-only flags and primary fields. |
| `homecore/devices/teslemetry_powerwall/state` | JSON object containing the latest published readings. |
| `homecore/devices/teslemetry_powerwall/availability` | `online` or `offline`. |

Powerwall registration, schema, state and availability use retained messages
with MQTT QoS 1. A new subscriber can receive the last retained value, which is
why state alone is not evidence of a fresh reading. A failed or stale poll
publishes `offline`; the next valid poll publishes state and then `online`.
The connection also has a retained `offline` last will, allowing the broker to
mark the device unavailable if the adapter disconnects unexpectedly.

Vehicles use the same device-topic layout with a device ID formed from
`teslemetry_vehicle_` and the SHA-256 hash of the VIN. Their registration
messages are not retained; schema, state and availability are retained. Each
vehicle has its own MQTT connection and offline last will.

### Vehicle fields

A vehicle is registered only after it appears in discovery. The adapter filters
identifier and credential-like field names, flattens supported nested readings
into names such as `location_latitude`, and builds a schema from the values it
has received. It does not invent a fixed set of sensors for every car.

Updates are ordered per field. An older snapshot may fill a field that has not
been seen, but cannot overwrite a newer value for that field. An explicit null
withdraws the field from both state and schema. Stream failure marks the vehicle
offline. If it disappears from a successful discovery response, its stream task
stops and the MQTT will marks it offline; the homeCore device record remains.
Location and other telemetry can still be private even though raw VINs and
vehicle IDs are filtered, so keep access to the broker and UI restricted.

## If readings are missing

Start with the [deployment health checks](../../deploy/README.md). For this
adapter, the common checks are:

| Symptom | Check |
| --- | --- |
| Process exits during startup | Required variables, numeric site ID and port, secret-file access and the loopback-only broker address. |
| Powerwall is unavailable | Teslemetry access, the host clock, source timestamp and validity of the live fields. The log deliberately reports only `Powerwall reading unavailable`. |
| Live power works but totals are missing or old | Account stream access and the date fields. Stream data arrives independently and is published with a later successful live poll. |
| No vehicle is registered | The account's vehicle list and permissions. Discovery waits 15 minutes between attempts. |
| A vehicle has only a few sensors | What Teslemetry has actually reported for that car. Check its telemetry configuration. |
| Adapter is running but homeCore has no readings | Broker credentials and the private plugin configuration. A running process alone does not establish successful MQTT delivery. |

Some Powerwall sensors require local pairing and are outside this adapter's
Teslemetry interface. The adapter also keeps no historical database; its stream
cache is memory only.

## Find the implementation

[`src/bin/teslemetry-bridge.rs`](src/bin/teslemetry-bridge.rs) reads configuration,
connects MQTT and runs the live polling loop.
[`src/teslemetry.rs`](src/teslemetry.rs) fetches and validates live readings;
[`src/totals.rs`](src/totals.rs) handles site events and energy totals;
[`src/vehicle.rs`](src/vehicle.rs) owns discovery and per-vehicle streams.
[`src/http.rs`](src/http.rs) supplies the ordinary HTTP clients.

The [broker contract tests](tests/teslemetry_adapter.rs) exercise the running
binary. The smaller source, totals and vehicle tests cover input validation
without Docker. To package a release for the trial, follow the
[image build instructions](../../deploy/README.md); the scratch image needs the
binary's runtime libraries as well as the binary itself.

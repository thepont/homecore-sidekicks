# Mitsubishi CN105 for homeCore

`mitsubishi-bridge` 0.2.0 connects to one or more controllers running
[`mitsubishi-cn105-homekit`](https://github.com/akifbayram/mitsubishi-cn105-homekit)
and connects them to HomeCore. Each AC has its own controls, telemetry
and diagnostics. The bridge connects directly to the controllers and
works alongside existing HomeKit pairings.

The supported source is **mitsubishi-cn105-homekit v0.2.8**, including Serin's
builds of that firmware. Serin's ESPHome and Matter builds use different
interfaces. Four NanoC6 controllers running
v0.2.8 were checked through their local WebSockets during development. Their
readings included compressor frequency, runtime and, on three controllers,
Govee temperature, humidity and battery levels. They did not supply electricity
consumption; outside temperature and error code were `null` on those units.
The final bridge was also checked against an isolated homeCore instance: all
four devices registered, with 83–92 available attributes per unit in that
snapshot.

## Configure and run

Copy [config.example.json](config.example.json) to a private configuration
directory. Replace the documentation addresses with your controllers' LAN
addresses or resolvable hostnames. Add or remove entries for your installation.
The bridge does not discover controllers or depend on multicast DNS.

```json
{
  "units": [
    {"id": "living_room", "name": "Living room AC", "url": "ws://192.0.2.10/ws"}
  ],
  "publish_interval_secs": 5,
  "stale_after_secs": 10
}
```

| Setting | Meaning |
| --- | --- |
| `units` | Between 1 and 32 controllers; IDs and endpoints must be unique. |
| `id` | Stable ID, at most 40 ASCII characters; start with a lowercase letter, then use lowercase letters, digits or `_`. Changing it creates a different homeCore identity. |
| `name` | Display name, at most 128 UTF-8 bytes, with no control characters. |
| `url` | `ws://host[:port]/ws`; default port 80. Credentials, query strings and fragments are rejected. |
| `publish_interval_secs` | Publish the latest reading every 1–60 seconds; default 5. Initial availability and loss of source data are reported promptly. |
| `stale_after_secs` | Stop trusting a stream that has supplied no valid state for 2–300 seconds; default 10. |

Unknown configuration keys are rejected. The configuration file is limited to
64 KiB. Timing values must be whole seconds. Restart the bridge after changing
the configuration.

Build from the repository root:

```sh
cargo build --release --locked --manifest-path services/mitsubishi/Cargo.toml --bin mitsubishi-bridge
```

Provide a dedicated broker username and its password in a private UTF-8 file.
The broker must already accept those credentials. Keep the configuration and
password outside version control. The password file may contain a trailing
newline; after trimming whitespace, the password must be nonempty printable
ASCII. The file limit is 4096 bytes.

```sh
export MITSUBISHI_CONFIG_FILE=/absolute/private/mitsubishi.json
export MQTT_HOST=127.0.0.1
export MQTT_PORT=1883
export MQTT_USERNAME=your-dedicated-broker-username
export MQTT_PASSWORD_FILE=/absolute/private/mqtt-password
services/mitsubishi/target/release/mitsubishi-bridge
```

All five environment variables are required. `MQTT_HOST` accepts only
`127.0.0.1` or `::1`; `MQTT_PORT` must be 1–65535. The username must be nonempty
printable ASCII and at most 128 bytes. The bridge must share the broker's
network namespace: for example, run it beside homeCore in the same Kubernetes
pod, or join the broker container's network namespace. A host process cannot
reach a pod's loopback broker through the host's `127.0.0.1`.

## Container packaging

[Dockerfile.mitsubishi](../../deploy/Dockerfile.mitsubishi) uses the same scratch
runtime approach as the existing Teslemetry image. It takes the **repository
root** as its build context and copies the release executable, a staged
tree of its shared libraries, and the [shared AC card](../../ui/ac-card/README.md).
It does not compile Rust inside Docker or require a Node runtime.

On the Linux build host, build the release binary, then stage its runtime
libraries. These Bash commands operate only on ignored build output:

```bash
set -euo pipefail
cargo build --release --locked --manifest-path services/mitsubishi/Cargo.toml --bin mitsubishi-bridge
runtime_dir="$PWD/services/mitsubishi/target/container-runtime"
mkdir -p "$runtime_dir"
ldd services/mitsubishi/target/release/mitsubishi-bridge \
  | awk '/=> \// {print $3} /^[[:space:]]*\// {print $1}' \
  | sort -u \
  | while IFS= read -r library; do
      install -D -m 0755 "$library" "$runtime_dir$library"
    done
tar -cf - deploy/Dockerfile.mitsubishi \
  services/mitsubishi/target/release/mitsubishi-bridge \
  services/mitsubishi/target/container-runtime \
  ui/ac-card/manifest.json ui/ac-card/index.js \
  | docker build -f deploy/Dockerfile.mitsubishi -t homecore-mitsubishi:0.2.0 -
```

Build on Linux for the target CPU architecture. The ELF loader and libraries
come from that build host; refresh the staged runtime when rebuilding. Start
with a fresh target directory when changing the build host or architecture.
The archive keeps repository-root paths while sending only the required build
files to Docker; source checkouts and private configuration are not included.
No production image digest or deployment change is supplied by this example.

Mount the configuration and password read-only and make both readable by
container UID 65532. The executable needs no writable application directory,
published port or Linux capability. For an existing Docker broker container:

```sh
docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges \
  --network container:YOUR_HOMECORE_CONTAINER \
  --mount type=bind,src=/absolute/private/mitsubishi.json,dst=/run/mitsubishi/config.json,readonly \
  --mount type=bind,src=/absolute/private/mqtt-password,dst=/run/mitsubishi/mqtt-password,readonly \
  -e MITSUBISHI_CONFIG_FILE=/run/mitsubishi/config.json \
  -e MQTT_HOST=127.0.0.1 -e MQTT_PORT=1883 \
  -e MQTT_USERNAME=your-dedicated-broker-username \
  -e MQTT_PASSWORD_FILE=/run/mitsubishi/mqtt-password \
  homecore-mitsubishi:0.2.0
```

The controller connection uses unencrypted local WebSockets. Restrict it to a
trusted LAN. The bridge sends climate commands through the firmware's
WebSocket control handler.

## How it works

```text
controller /ws -> source supervisor -> latest-value channel -> MQTT publisher
                        |
                  validated model
```

The protocol **Adapter** in `model.rs` turns firmware fields into homeCore
readings. `source.rs` supervises each connection and its freshness deadline.
The publisher observes a bounded latest-value channel, an **Observer** pattern
that lets a slow broker skip superseded snapshots without blocking the device
reader. `config.rs` checks identities, endpoints and limits before tasks start.
These are separate modules with small interfaces; adding another device does
not add another process or another copy of the protocol logic.

For every configured unit, the bridge keeps one WebSocket connection and one
MQTT connection. The firmware automatically sends `deviceInfo` and `state`
messages after connection, then pushes state regularly. The adapter validates
each message at this boundary and projects explicitly allowed fields into the
homeCore sensor schema.

A latest-value channel connects the source reader to the publisher. It holds
the newest observation rather than queuing a history of every firmware update.
Each valid state replaces the previous state in full: fields that are absent
or `null` disappear from both the values and schema. Device information is kept
for that WebSocket session and refreshed when another `deviceInfo` arrives.
Metadata and log messages do not extend reading freshness.

Each unit reconnects independently, with delays increasing from 1 to 30 seconds.
WebSocket connection attempts have a five-second timeout, and incoming frames
and messages are limited to 32 KiB. Invalid known values or malformed messages
close that source session. Unrecognised message types are ignored.

When a WebSocket disconnects or valid state stops arriving before the stale
deadline, the bridge publishes empty state and schema and marks the device
offline. A reported `connected:false` also marks the device offline: the
controller's Wi-Fi connection does not prove its CN105 connection is working.
Consumers must respect availability before presenting readings as current.

MQTT reconnects restore registration, schema and the latest valid state. Each
connection has its own retained `offline` last will. If the bridge process dies,
the broker marks each device offline, but its last retained state can remain;
the last will cannot also clear a separate state topic. A normal running bridge
clears state when the source is lost. The MQTT keep-alive interval is 15 seconds,
so abrupt process or network failure is not necessarily detected immediately.
Delivery also depends on the broker: a connected broker that stops accepting
publications can delay both readings and offline notices. The bridge's source
reader stays bounded while MQTT publication is delayed.

## homeCore identities and topics

For configured ID `living_room`, the plugin is `plugin.mitsubishi.living_room`
and the device is `mitsubishi_living_room`. Do not run two bridge processes
with the same IDs against the same broker: their MQTT client identities collide.

| Topic suffix | Prefix | Content |
| --- | --- | --- |
| `register` | `homecore/plugins/plugin.mitsubishi.living_room/` | Device name, IDs and sensor type; retained. |
| `capabilities` | Same plugin prefix | Empty action list and the AC card descriptor when assets are available; retained. |
| `manage/cmd`, `manage/response` | Same plugin prefix | HomeCore requests and replies for the shared card bundle. |
| `heartbeat` | Same plugin prefix | Adapter version and uptime, every 30 seconds. |
| `schema` | `homecore/devices/mitsubishi_living_room/` | Attribute types, units and writable capabilities; retained. |
| `cmd` | Same device prefix | One writable attribute update from HomeCore. |
| `state` | Same device prefix | Latest permitted telemetry; retained. |
| `availability` | Same device prefix | `online` or `offline`; retained. |

The bridge uses each unit’s existing HomeCore sensor identity. The shared
card supplies controls on standard and large cards on Home and room pages.
Tap **Open controls** on the room temperature reading to use the larger dial
in the HomeCore device sheet.

The container serves the two asset files from `/ui`. In a local checkout set
`HC_PLUGIN_UI_DIR="$PWD/ui/ac-card"` when starting the bridge. Missing or
invalid assets leave the device available with HomeCore’s default card.

## Controls

The card shows the modes configured on the controller. Select one to turn the
unit on, or choose **Off**. HomeCore users with device command permission can
also adjust the temperature targets and supported airflow settings.

| Writable attribute | Accepted values |
| --- | --- |
| `hvac_mode` | `off` or a supported mode from `heat`, `cool`, `auto`, `dry`, `fan_only`. |
| `setpoint` | In Heat or Cool, 16–30 °C in whole degrees for legacy and enhanced CN105 units. |
| `heat_setpoint`, `cool_setpoint` | In Auto, 16–30.5 °C in half-degree steps, with at least 2 °C between targets. |
| `fan_mode`, `vertical_vane`, `horizontal_vane` | The options in the unit’s schema, while the unit is on. |

The bridge checks the latest available controller state before sending each
command. It waits for the same unit to echo the requested setting before
accepting another while the write is pending. Retained MQTT replays, offline
units and invalid requests are ignored. The card reflects the controller’s
reported state. This firmware can briefly echo a requested value before the
indoor unit applies it. Any later correction from the controller also reaches
the card.

## Readings and diagnostics

Fields appear only when the firmware supplies usable values. Zero is a valid
reading where the source permits it; missing data is never converted to zero.
Temperatures from this firmware are Celsius even when its display preference
is Fahrenheit.

| Firmware field | homeCore attribute | Unit or meaning |
| --- | --- | --- |
| `connected`, `power`, `operating` | `cn105_connected`, `power`, `operating` | Connection and actual boolean states. |
| `room`, `target` | `temperature`, `setpoint` | °C. The target is writable in Heat or Cool. |
| `mode`, `fan` | `operation_mode`, `fan_mode` | Source mode and fan setting. |
| Power, mode and activity combined | `hvac_mode`, `hvac_action` | Derived display states; an off AC is `off` even if its remembered source mode is `cool`. Transitional states do not claim active heating/cooling. |
| `vane`, `wideVane` | `vertical_vane`, `horizontal_vane` | Source vane settings. |
| `vaneConfig` | `has_vertical_vane`, `has_horizontal_vane` | Optional read-only capability flags. |
| `subMode`, `stage`, `autoSubMode` | `sub_mode`, `stage`, `auto_sub_mode` | Operating detail. |
| `outsideTemp`, `heatThresh`, `coolThresh` | `outside_temperature`, `heating_setpoint`, `cooling_setpoint` | °C; optional. |
| `compressorHz`, `runtime`, `errorCode` | `compressor_frequency`, `runtime`, `fault`, `error_code` | Hz, operating hours, explicit fault state and raw fault code. These are not electrical watts or energy. |
| `tempUnit` | `display_temperature_unit` | Controller display preference, `C` or `F`. |
| `rssi`, `wifiUptime`, `wifiDrops` | `wifi_rssi`, `wifi_uptime`, `wifi_disconnects` | dBm, seconds, count. |
| `cn105Baud`, `cn105Drops`, `pollInterval` | `cn105_baud`, `cn105_disconnects`, `poll_interval` | Baud, count, milliseconds. |
| `uptime`, `epoch` | `uptime`, `device_epoch` | Seconds; an unset zero epoch is omitted. Freshness uses local receipt time. |
| `heapFree`, `heapMin`, `heapBlock` | `heap_free`, `heap_minimum`, `heap_largest_block` | Bytes. |
| `crashCount`, `resetReason` | `crash_count`, `reset_reason` | Controller diagnostics. |
| `board`, `fw`, `idf`, `bootCount`, `crashTotal`, `safeMode` in `deviceInfo` | `board`, `firmware`, `idf_version`, `boot_count`, `crash_total`, `safe_mode` | Firmware and boot information. |

BLE sensors use their firmware slot `i`, from 0 to 3, as
`ble_<i>_temperature`, `humidity`, `battery`, `rssi`, `age`, `type`, `active`,
`stale` and `available`. For example, slot 1 is `ble_1_temperature`; the first
array entry need not be slot 0. Temperature is °C, humidity/battery are %, RSSI
is dBm, and age is milliseconds. Inactive, stale or disabled sensors lose their
measurements. The source's battery `-1` sentinel is omitted. The legacy flat BLE
readings are not duplicated alongside the slot readings.

Room-source diagnostics include `room_source`, `room_status`, `room_mode`,
`room_single`, `room_members`, `room_contributors`, `room_excluded_stale`,
`room_excluded_disabled`, `room_feed_active`, `room_feed_fallback`,
`effective_temperature_age` and `temperature_spread`. Selector and mask fields
retain their firmware values. `room_offset_0` through `room_offset_5` convert
the firmware's tenths of a degree to °C. `effective_temperature` is supplied
only while the source reports `avgFeeding:true`.

Link readings include `link_sensor_present`, `link_active`, `link_stale`,
`link_age`, `link_temperature` and `link_humidity`. Temperature and humidity
require an active, fresh Link sensor. Remote controller metadata/status uses
`remote_bonded`, `remote_live`, `remote_rssi`, `remote_model`, `remote_firmware`,
`remote_last_seen`, `remote_syncing`, `remote_certificate_state`,
`remote_pairing`, `remote_pairing_remaining` and `remote_pairing_result`.
`remote_dial_count` and `remote_dial_<n>_{live,last_seen,temperature}` describe
up to four entries in the firmware's current dial-array order; that index is
not a permanent dial identity. Missing last-seen sentinel values are omitted,
and offline dials lose temperature readings.

Additional reported configuration/status fields are `mode_capabilities`,
`boot_mode_capabilities`, `vane_configuration`, `sensor_timeout`,
`homekit_ready`, `homekit_paired`, `homekit_status`, `homekit_controllers`,
`ble_enabled`, `ble_discovering`, `ble_reverted`, `beta_channel`, `log_level`,
`night_dusk_offset`, `night_dawn_offset`, `night_ceiling` and
`snapshot_truncated`. These are observations, not configuration controls.

Pairing codes and setup URIs, Wi-Fi names, IP/MAC addresses, location coordinates,
raw logs and arbitrary source fields are excluded. The model's exact mapping
and validation are in [src/model.rs](src/model.rs); the [field catalogue](docs/fields.md)
explains fault states and calibration indices. Unsupported firmware fields
require a reviewed mapping before they can appear in homeCore.

## Troubleshooting and checks

If a device never appears, check that the configuration parses, the mounted
files are readable by the process user, and MQTT credentials work in the same
network namespace. If it appears offline, check the controller address,
firmware family and CN105 connection. HomeKit availability alone does not prove
that the configured WebSocket endpoint is reachable from this service.

A field that disappears may have been withdrawn, become stale or failed
validation. Outside temperature, fault codes, BLE and Link readings depend on
the unit and its attached sensors. A missing watts/kWh field cannot be recovered
from compressor frequency or runtime without another measurement source.

```sh
cargo test --locked --manifest-path services/mitsubishi/Cargo.toml
cargo test --locked --manifest-path services/mitsubishi/Cargo.toml --test bridge -- --ignored --test-threads=1
cargo clippy --locked --manifest-path services/mitsubishi/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path services/mitsubishi/Cargo.toml --check
```

The MQTT integration tests use their own temporary, authenticated Mosquitto
container bound to loopback and fake WebSocket controllers. They require a
working Docker daemon and the pinned fixture image. Live controller checks
must use a separate temporary broker and a deadline, with no application
commands sent to the controller.

## Source contract

- [Serin firmware distribution and firmware families](https://github.com/Serin-Labs/serin-cn105).
- [HomeKit firmware v0.2.8 source, revision `56676c8`](https://github.com/akifbayram/mitsubishi-cn105-homekit/tree/56676c8fad6053be23d80e3b02d9089ae559a2a3).
- [WebSocket state and device-information implementation at that revision](https://github.com/akifbayram/mitsubishi-cn105-homekit/blob/56676c8fad6053be23d80e3b02d9089ae559a2a3/main/web_ws.cpp).
- [HTTP/WebSocket endpoint registration at that revision](https://github.com/akifbayram/mitsubishi-cn105-homekit/blob/56676c8fad6053be23d80e3b02d9089ae559a2a3/main/web_server.cpp).

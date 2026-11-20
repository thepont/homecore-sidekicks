# Electrolux Climate adapter

This experimental bridge reads and controls one Electrolux Climate controller over its local Broadlink interface, device type `0x4f9b`. It publishes to homeCore over a local MQTT broker. It has not yet replaced the existing Home Assistant integration or its automations.

The bridge supports power, whole-degree target temperature, HVAC mode, fan speed and vertical swing. It polls the controller every five seconds and publishes the status returned by the device after each command. A failed read or command marks the device offline; a retained MQTT last will marks it offline if the process disconnects. The broker and controller must be on trusted networks: Broadlink uses a shared protocol bootstrap key and does not provide modern authenticated transport.

## Configure

Run the bridge beside the homeCore broker. Keep the MAC address, controller address and broker credential in private configuration. The MAC is 12 hexadecimal digits without separators. The broker accepts only IPv4 loopback; the controller address includes its UDP port, normally 80. Choose a dedicated broker account permitted to use this plugin's topics.

```sh
export ELECTROLUX_ADDRESS=192.0.2.10:80
export ELECTROLUX_MAC=020000000001
export MQTT_HOST=127.0.0.1
export MQTT_PORT=1883
export MQTT_USERNAME=plugin.electrolux.living_room
export MQTT_PASSWORD_FILE=/absolute/private/path/mqtt-password
cargo run --release --locked --bin electrolux-bridge
```

The fixed plugin and device identifiers are `plugin.electrolux.living_room` and `electrolux_living_room`. Run only one bridge with these identifiers on a broker. The credential file is bounded to 4096 bytes and must contain printable ASCII with optional surrounding whitespace. Commands are limited to one recognized field, 1024 bytes and a bounded queue. Commands delivered with the MQTT retained flag and commands received while the device is unavailable are ignored. Setpoints must be whole degrees between 17 and 30°C, matching the existing installation.

The broker command topic is `homecore/devices/electrolux_living_room/cmd`. Supported fields are `on` (boolean), `setpoint` (whole degrees Celsius), `hvac_mode` (`off`, `cool`, `heat`, `dry`, `fan_only`, `auto`, `heat_cool`), `fan_mode` (`auto`, `low`, `medium`, `high`, `turbo`, `quiet`), and `swing` (boolean). Only one field can be sent in each message. Commands may be dropped when the bounded queue is full. Successful submission is not confirmation: the subsequent device state is authoritative. Restrict broker access to authorized publishers.

## Build and verify

```sh
cargo test --locked
cargo test --locked --test mqtt -- --ignored --test-threads=1
cargo build --release --locked --bin electrolux-bridge
```

The MQTT tests require Docker and the pinned Mosquitto image named in `tests/support/mqtt_broker.rs`; they use a simulated AC and isolated broker. `tests/live.rs` is an opt-in, read-only hardware test and requires `ELECTROLUX_LIVE_CONFIG` to point to a private JSON file containing `host` and `mac`. Never commit the file. A read-only status check succeeded against the existing living-room AC; real-device control and end-to-end homeCore control have not yet been verified.

The Electrolux frame follows the installed [Electrolux Climate component](https://github.com/jnctech/ElectroluxClimateComponent), and the bridge uses pinned `rbroadlink` for Broadlink packet encryption. Its dependency audit currently reports unmaintained transitive `chrono` and `gcc` crates. Review those dependencies before deployment.


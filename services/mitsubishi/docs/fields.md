# Published readings

The adapter reads mitsubishi-cn105-homekit's `state` and `deviceInfo`
WebSocket frames. This catalogue follows [v0.2.8, commit
56676c8](https://github.com/akifbayram/mitsubishi-cn105-homekit/blob/56676c8fad6053be23d80e3b02d9089ae559a2a3/main/web_ws.cpp).
Only the climate settings listed in the [control guide](../README.md#controls)
can be written. Unsupported, absent and null readings
are omitted; a new snapshot replaces the previous snapshot.

| Group | Attributes | Units or meaning |
| --- | --- | --- |
| Climate | `temperature`, `setpoint`, `outside_temperature`, `heating_setpoint`, `cooling_setpoint` | °C, regardless of the controller's display unit |
| Operating state | `power`, `operating`, `operation_mode`, `hvac_mode`, `hvac_action`, `fan_mode` | Booleans or reported modes; see below |
| Airflow | `vertical_vane`, `horizontal_vane`, `stage`, `sub_mode`, `auto_sub_mode` | Firmware's named values |
| Heat pump | `cn105_connected`, `compressor_frequency`, `runtime`, `fault`, `error_code` | Boolean, Hz, accumulated hours, fault boolean, numeric error code |
| Controller | `uptime`, `wifi_uptime`, `wifi_rssi`, `heap_free`, `heap_minimum`, `heap_largest_block` | Seconds, seconds, dBm, bytes, bytes, bytes |
| Health | `reset_reason`, `crash_count`, `wifi_disconnects`, `cn105_disconnects`, `device_epoch`, `snapshot_truncated` | Cause, counts, Unix seconds, boolean; epoch zero is unknown and omitted |
| Observed configuration | `poll_interval`, `cn105_baud`, `display_temperature_unit`, `vane_configuration`, `mode_capabilities`, `boot_mode_capabilities`, `log_level`, `beta_channel` | ms, baud, C/F, firmware configuration values |
| Night settings | `night_dusk_offset`, `night_dawn_offset`, `night_ceiling` | Minutes, minutes, percent; location coordinates are excluded |
| Room sensing | `room_source`, `room_status`, `room_mode`, `room_single`, `room_members`, `room_contributors`, `room_excluded_stale`, `room_excluded_disabled` | Firmware's source/status values and member masks |
| Room feed | `room_feed_active`, `room_feed_fallback`, `effective_temperature`, `temperature_spread`, `effective_temperature_age`, `sensor_timeout` | Booleans, °C, °C difference, ms, seconds |
| Link sensor | `link_sensor_present`, `link_active`, `link_stale`, `link_temperature`, `link_humidity`, `link_age` | Booleans, °C, percent, ms |
| Link connection | `remote_bonded`, `remote_live`, `remote_syncing`, `remote_last_seen`, `remote_rssi`, `remote_certificate_state`, `remote_model`, `remote_firmware` | Booleans, seconds, dBm, firmware metadata; last-seen -1 is omitted |
| Link pairing status | `remote_pairing`, `remote_pairing_remaining`, `remote_pairing_result` | Boolean, seconds, status text; no pairing command is sent |
| HomeKit status | `homekit_ready`, `homekit_paired`, `homekit_controllers`, `homekit_status` | Status only; pairing codes and setup URI are excluded |
| BLE status | `ble_enabled`, `ble_discovering`, `ble_reverted` | Firmware status booleans |
| Device information | `board`, `firmware`, `idf_version`, `reset_reason`, `boot_count`, `crash_total`, `safe_mode` | Connection-time firmware and health metadata |

`hvac_mode` is `off` when power is off; otherwise it uses `heat`, `cool`,
`dry`, `auto` or `fan_only`. `operation_mode` preserves the underlying mode
even when power is off. `fan_mode` preserves `auto`, `quiet`, and speeds `1`
through `4`, without assigning model-dependent speed labels.

`hvac_action` uses power, operating status and mode. Auto mode needs the
reported heating/cooling submode. An active defrost, preheat, standby or
unknown submode does not produce a guessed heating/cooling action. It is
omitted when that evidence is insufficient. Controller connectivity and
CN105 connectivity remain separate concerns.

BLE readings use the reported slot, not the array position. For example,
a sensor in slot 1 publishes `ble_1_temperature`, `ble_1_humidity`,
`ble_1_battery`, `ble_1_rssi`, `ble_1_age`, `ble_1_type`, `ble_1_active`,
`ble_1_stale` and `ble_1_available`. Slots range from 0 to 3. Units are °C,
percent, percent, dBm and milliseconds. Inactive, stale or globally disabled
sensors retain their status and age but omit temperature, humidity, battery
and signal readings. Battery -1 means unknown and is omitted. Legacy flat
BLE readings duplicate the sensor list and are not published separately.

`room_offset_0` through `room_offset_5` convert calibration offsets to °C.
Index 0 is the internal thermistor, 1 is Link, and 2–5 correspond to BLE
slots 0–3. The same indices define the room member masks. These assignments
come from the firmware's [settings contract](https://github.com/akifbayram/mitsubishi-cn105-homekit/blob/56676c8fad6053be23d80e3b02d9089ae559a2a3/main/settings.h#L27).

`remote_dial_count` reports the number of paired dials. Each current list
position publishes `remote_dial_<index>_live`, `_last_seen` (seconds) and
`_temperature` (°C when live). These indices are positions in the controller's
bond list and can change when a dial is removed; network addresses are not
published. Link sensor measurements are omitted while inactive or stale,
and effective room temperature is omitted when the room feed is inactive.

`snapshot_truncated:true` means the firmware omitted one or more optional
sections. Missing readings are removed rather than carried forward. Neither
this firmware nor the adapter provides watts or kWh; compressor frequency
and runtime are separate measurements.

`fault:false` means the controller explicitly reported no error (`errorCode:null`).
A numeric error publishes `fault:true` and `error_code`; if the error field is
absent, fault status is unknown and both attributes are omitted.

The parser accepts at most 32 KiB per message, validates supported fields and
array sizes, ignores unrelated message types and unknown fields, and emits
fixed error messages. Raw frames, logs, network names/addresses, sensor names,
location coordinates and HomeKit setup secrets are never published.

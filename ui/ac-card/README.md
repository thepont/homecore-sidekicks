# HomeCore AC card

This is a shared HomeCore `device.card` bundle. Serve `manifest.json` and
`index.js` from each AC plugin's UI directory. The bundle has no brand or
device IDs, build step, external asset requests, or browser access to the
controller. The HomeCore host supplies the plugin's device, permissions,
commands, display units and theme. The manifest opts in to standard and large
cards. Compact cards use HomeCore's own rendering.

A plugin should advertise this slot for AC devices. For a mixed-device plugin,
include a `device_types` filter in its own manifest.

## Device contract

| Field | Meaning |
| --- | --- |
| `temperature` | Room reading in °C. |
| `hvac_mode` | `off`, `heat`, `cool`, `auto`, `dry`, or `fan_only`. |
| `hvac_action` | Optional reported activity, such as `idle`, `heating`, `cooling`, `drying`, `fan`, or `off`. |
| `sub_mode` | Optional `defrost`, `preheat`, or `standby` activity in Heat or Auto. |
| `setpoint` | Single target in °C. |
| `heat_setpoint`, `cool_setpoint` | Auto range in °C when both are reported. A device with one target can use `setpoint` in Auto. The existing `heating_setpoint` and `cooling_setpoint` aliases are accepted for display only. |
| `min_setpoint_gap` | Optional minimum separation of Auto targets in °C. Defaults to zero. |
| `fan_mode`, `vertical_vane`, `horizontal_vane` | Optional airflow settings. |
| `has_vertical_vane`, `has_horizontal_vane` | Optional capability flags. An explicit false hides that vane even if a setting is reported. |
| `outside_temperature`, `compressor_frequency`, `runtime` | Optional diagnostics in °C, Hz, and hours. |

The current room temperature appears below the dial in whole degrees, using
the person’s display units. Targets retain the adapter’s precision.

The card suppresses retained readings when HomeCore reports a device offline.
Off can still show a current room reading. Off, Dry and Fan only omit
temperature targets. The Airflow row goes below the mode controls and adapts to
the fields the device supports. The device name opens the HomeCore device panel.

Controls require command permission and schema attributes marked
`writable: true`. Mode and airflow controls use schema `options` (strings or
objects with string `value` and optional `label`). Target controls require an
explicit `min`, `max` and positive `step` in °C. The card sends the raw value
through `host.command` and keeps the reported display until the device echoes
the requested value. Adapters must enforce command validity and interlocks
themselves because commands may also come from outside the card.

Mitsubishi CN105 is the first consumer. Its adapter publishes read-only
readings today.

## Tests

The browser tests use Playwright and a HomeCore host fixture for both read-only
and writable ACs. Use an installed Playwright or set `NODE_PATH` to an existing
Playwright installation. Set `CHROMIUM_BIN` if using a separately installed
browser.

```sh
node --test ui/ac-card/tests/card.test.mjs
```

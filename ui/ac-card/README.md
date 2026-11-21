# HomeCore AC card

This is a shared HomeCore `device.card` and `device.panel` bundle. Serve `manifest.json` and
`index.js` from each AC plugin's UI directory. The bundle has no brand or
device IDs, build step, external asset requests, or browser access to the
controller. The HomeCore host supplies the plugin's device, permissions,
commands, display units and theme. The manifest opts in to standard and large
cards. Compact cards use HomeCore's own rendering.

A plugin should advertise these slots for AC devices. For a mixed-device plugin,
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

The thermostat dial shows the target above the current room temperature. With
separate Auto targets, it shows the room temperature in the centre and the two
targets below the dial. The room reading uses whole degrees in the person’s
display units. Targets retain the adapter’s precision.

The card suppresses retained readings when HomeCore reports a device offline.
Off can still show a current room reading and a reported target. Target controls
are unavailable in Off, Dry and Fan only. Airflow appears below the mode
controls and adapts to the fields the device supports. Tap the dial readout or
**Open controls** under the device name for the larger dial in HomeCore’s device
sheet.

Controls require command permission and schema attributes marked
`writable: true`. Mode and airflow controls use schema `options` (strings or
objects with string `value` and optional `label`). Target controls require an
explicit `min`, `max` and positive `step` in °C. The card sends the raw value
through `host.command` and keeps the reported display until the device echoes
the requested value. Drag a target marker to preview a temperature and release
to send it. The dial and step buttons share the same temperature limits. Use
the arrow keys to adjust a focused marker and Escape to cancel a drag.
On a touchscreen, drag along the arc. The closest target moves in Auto.

Adapters must enforce command validity and interlocks themselves because
commands may also come from outside the card.

Mitsubishi CN105 is the first consumer. Its adapter sends commands to each
controller and advertises the controls supported by that unit.

## Tests

The browser tests use Playwright and a HomeCore host fixture for both read-only
and writable ACs. Use an installed Playwright or set `NODE_PATH` to an existing
Playwright installation. Set `CHROMIUM_BIN` if using a separately installed
browser.

```sh
node --test ui/ac-card/tests/card.test.mjs
```

## Shared package

This directory is the private `@homecore/ac-card` package. Its `index.js` and
`manifest.json` are the same runtime files consumed by Mitsubishi and Electrolux;
there is no separate copy or browser build. Run `npm pack --dry-run` here to
inspect the package contents. Each adapter includes those two files in its
image and exposes them through its own homeCore plugin identity.

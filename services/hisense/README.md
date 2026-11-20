# Hisense TV sensor

A read-only homeCore `media_player` for Hisense VIDAA TVs that expose the local
UPnP RenderingControl service. It polls `GetVolume` and `GetMute`; it does not
send TV control commands or use the Home Assistant pairing credentials. The
reported fields are `volume` (0–100 percent) and `muted` (boolean). UPnP
reachability does not prove whether a screen is on: if a reading fails the
device becomes unavailable, not “off”. This first slice does not report HDMI,
Cast playback, power, or source.

The service connects to homeCore's local MQTT broker as `plugin.hisense` and
registers `hisense_living_room`. The broker login must be provisioned separately;
use a dedicated password stored in a secret file. Credentials and the TV IP do
not belong in version control.

Required environment:

- `HISENSE_IP`: TV IPv4 or IPv6 address, for example `192.0.2.50`.
- `HISENSE_PORT`: UPnP port, commonly `18400` or `38400`, confirmed on the device.
- `HISENSE_POLL_SECS`: polling interval from 5 to 300 seconds.
- `MQTT_HOST`: `127.0.0.1` or `::1`, the local homeCore broker.
- `MQTT_PORT`: broker TCP port.
- `MQTT_USERNAME`: provisioned broker username.
- `MQTT_PASSWORD_FILE`: file containing the broker password.

For tests, run `cargo test --locked`. The broker contract is opt-in because it
starts a pinned Mosquitto container: `cargo test --locked --test mqtt -- --ignored`.
The hardware check is read-only and opt-in: set `HISENSE_LIVE_ADDRESS` to the
TV's IP and port, then run `cargo test --locked --test status
reads_the_real_tv_without_sending_commands -- --ignored`.

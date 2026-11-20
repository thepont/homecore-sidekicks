# Hisense TV sensor

A homeCore `media_player` for Hisense VIDAA TVs that expose the local UPnP
RenderingControl service. It polls `GetVolume` and `GetMute`, and accepts
absolute volume (0–100 percent) and mute commands using `SetVolume` and
`SetMute`. It does not use Home Assistant pairing credentials. State is updated
from subsequent TV readings, not assumed from command submission. UPnP
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
The hardware status check is read-only and opt-in: set `HISENSE_LIVE_ADDRESS` to the
TV's IP and port, then run `cargo test --locked --test status
reads_the_real_tv_without_sending_commands -- --ignored`.

## Deployment

Build the static release binary with a pinned Alpine/musl Rust toolchain and
build `deploy/Dockerfile.hisense` from the repository root. Publish the resulting
image to the registry used by the node that will run it, then pin that registry's
manifest digest. Separate node-local registries can assign different manifest
digests to the same image; verify the digest on each node.

The running sensor uses a separate `plugin.hisense` broker login. Core supports
plugin credentials in `config/plugins/plugin.hisense.toml` with a `[homecore]`
`password` and reads the password when a connection is made. Store the same
password as the `mqtt-password` key of a Kubernetes Secret. Provision secrets
through the deployment's secret-management process; don't commit values. The
broker connection in the example assumes the sensor shares a pod and network
namespace with homeCore.

`deploy/hisense-sidecar.example.yaml` is a container and volume fragment, not a
complete deployment. Replace its documentation-only IP and image reference,
create the secret, and merge the fragment into the **current** homeCore pod
specification. Check the current deployment generation before applying a patch,
so unrelated changes are not overwritten. Updating a pod template restarts the
pod; the core's in-memory recent-event buffer starts afresh.

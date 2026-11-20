# homeCore HomeKit controller (in development)

This crate is a HomeKit IP controller for HomeCore. Its setup screen, management
actions, contact and battery readings, and homeCore MQTT publications have
been exercised against simulated accessories and an isolated homeCore. A fake
bridge has also accepted a second controller without removing the first one.

The Xiaomi DMWG03LM hub has accepted a second, non-admin controller alongside
Home Assistant, and both controller identities read the same contact and
battery values. This hub rejected controller identifiers longer than 36 bytes;
new controller identities use UUIDs. Kubernetes discovery and live deployment
still need verification. The current implementation handles contact sensors
and their batteries; it does not yet cover temperature, humidity, smoke alarms,
or outlets. Do not transfer or remove the existing Home Assistant pairing until
the remaining devices and recovery path are verified.

The pairing test starts a HAP-NodeJS accessory bound to loopback, with an
isolated temporary identity. Node.js 24 and the pinned fixture dependencies are
required:

```sh
npm ci --prefix tests/hap --ignore-scripts
HAP_NODE_BIN=node cargo test --locked --test pairing
cargo test --locked --test contact
```

The local `hap-transport` patch and its upstream source are documented in
[vendor/hap-transport/PATCH.md](vendor/hap-transport/PATCH.md). Pairing uses a
private state directory. Tests cover failed PINs, credential permissions,
interrupted pairing, contact and battery updates, and recovery after simulated
bridge and broker restarts. Browser tests are in the Xiaomi UI test directory;
the separate homeCore UI contract test requires Docker and the pinned image.

## HomeCore package

Build the Linux x86-64 archive from the repository root. The build requires the
Rust musl target and Rust documentation component, a musl C compiler, GNU tar,
zstd, and cargo-about 0.9.3. With Zig as the C compiler, pass
`CFLAGS_x86_64_unknown_linux_musl=-fno-sanitize=all` so its C objects link to
the Rust musl executable.

```sh
sh deploy/package-homekit.sh
HOMEKIT_TEST_ARCHIVE="$PWD/.local/packages/plugin.homekit-0.1.0-linux-x86_64.tar.zst" \
  cargo test --locked --manifest-path services/homekit/Cargo.toml \
  --test homecore_ui_contract installs_the_signed_homekit_package -- --ignored
```

HomeCore installs the package as `plugin.homekit`. The setup page is available
on first start. The optional `[homekit].refresh_secs` setting defaults to 30 and
controls how often the adapter reconciles sensor snapshots. HomeCore supplies
the MQTT credentials. Pairing data lives in the private `plugin.homekit.state`
directory beside the managed TOML, outside the versioned package. Keep the
directory when upgrading to preserve the controller identity.

This package can run alongside the existing Mi Home setup sidecar. A fresh
HomeKit setup uses the accessory's pairing code. An already paired hub needs a
second controller invited by its current administrator. The setup page supports
initial pairing. The real Xiaomi hub's second read-only controller was verified
separately from the packaged HomeCore install.

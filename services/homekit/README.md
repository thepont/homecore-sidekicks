# homeCore HomeKit controller (in development)

This crate is an undeployed HomeKit IP controller. Its setup screen, management
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

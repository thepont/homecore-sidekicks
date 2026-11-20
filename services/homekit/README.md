# homeCore HomeKit controller (in development)

This crate currently pairs with a HomeKit IP accessory and reads contact and
battery characteristics in an isolated contract test. It does not yet publish
devices to homeCore or provide a setup screen, and it is not deployed.

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
private state directory; the test checks credential-file permissions and a
failed-PIN retry. A real hub should not be unpaired until discovery, events,
restart recovery, the homeCore interface, and a safe transfer path are tested.

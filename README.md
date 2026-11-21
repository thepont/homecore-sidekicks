# homeCore Sidekicks

Device integrations for [homeCore](https://homecore.io/). Each service
publishes device state and capabilities through the homeCore MQTT protocol.

| Service | What it provides |
| --- | --- |
| [Teslemetry](services/teslemetry/README.md) | Read-only Powerwall, solar, grid, Wall Connector and vehicle telemetry, plus energy cards and [tariffs](docs/tariffs.md). |
| [Mitsubishi CN105](services/mitsubishi/README.md) | Climate controls, temperatures, airflow, external sensors and controller diagnostics. |
| [Electrolux](services/electrolux/README.md) | Broadlink climate readings and controls. |
| [Hisense](services/hisense/README.md) | Read-only TV volume and mute via UPnP. |
| [HomeKit](services/homekit/README.md) | Pairing and readings for supported HomeKit accessories. |
| [Xiaomi](services/xiaomi/README.md) | Shared Xiaomi sign-in, device discovery and authorised local adapter setup. |

Mitsubishi and Electrolux use the same [AC card](ui/ac-card/README.md).
Teslemetry includes Powerwall and energy dashboard cards. Its vehicle adapter
discovers cars without waking them. See the [device integration guide](docs/homecore-device-integration.md)
for the standard device types and schemas.

## Configure and deploy

Start with the service guide and the [deployment guide](deploy/README.md).
Keep installation configuration, addresses, device identifiers, runtime state
and credentials in private files or a secret store.

## Build and test

Each service is a separate Rust package with a lockfile. Use Rust and Cargo to
build it. Docker is required for the broker and HomeCore contract tests and the
pre-commit secret scan.

```sh
git config core.hooksPath .githooks
cargo test --locked --manifest-path services/teslemetry/Cargo.toml
cargo test --locked --manifest-path services/teslemetry/Cargo.toml --test homecore_ui_contract -- --ignored
```

The service guides list their other local test commands and opt-in hardware
tests. The HomeCore contract tests start an isolated, pinned HomeCore image.
For the Teslemetry card browser tests, use the test package from a checkout.

```sh
cd services/teslemetry/tests/ui
npm ci
npx playwright install chromium
npm test
```

The commit hook scans the staged tree with a pinned TruffleHog image on an
isolated Docker network.

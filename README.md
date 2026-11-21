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
For the Teslemetry card browser tests, install Chrome and use the test package from a checkout.

```sh
cd services/teslemetry/tests/ui
npm ci
CHROMIUM_BIN="$(command -v google-chrome-stable || command -v google-chrome)" npm test
```

The commit hook scans staged files using a pinned TruffleHog container with
network access disabled. CI runs the same scan on the checked-out tree and
checks Rust formatting, Clippy warnings, tests and release builds for all six
services. It also builds the Xiaomi extractor with only its own features.

CI compares Rust readability warnings and Rust/JavaScript function complexity
with the pull request's base revision. New or increased findings fail the
check; existing findings remain as candidates for refactoring. Complexity
limits are 15 cognitive and 12 cyclomatic, with existing higher scores allowed
only up to their base values. Clippy compares counts per file and rule, so a
replacement finding can be masked by another finding being removed in the same
file. CI also uploads Bluecheese duplicate-code and long-method reports for
review; these reports are not automatic failures.

JavaScript uses a pinned ESLint and records existing violations per file and
rule in `tools/javascript-quality/eslint-suppressions.json`. The baseline
includes complexity, long functions, short names, unused variables, magic
numbers and suspicious `for` loops. CI rejects increased counts. To run it:

```sh
npm ci --prefix tools/javascript-quality --ignore-scripts
node tools/javascript-quality/node_modules/eslint/bin/eslint.js \
  --config tools/javascript-quality/eslint.config.mjs \
  --suppressions-location tools/javascript-quality/eslint-suppressions.json \
  'services/*/ui/**/*.js' 'ui/**/*.js' --ignore-pattern '**/tests/**'
```

After removing existing JavaScript violations, update the recorded counts with
the same command plus `--prune-suppressions` and commit the result. Rust Clippy
does not provide a general magic-number rule; review those as refactoring
candidates. Third-party and test fixtures are excluded from the complexity and
Bluecheese reports.

The dependency security workflow audits the Rust and npm lockfiles on dependency
changes and once a week. Dependabot proposes weekly updates for Rust, npm and
GitHub Actions dependencies.

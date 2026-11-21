# Deploying the services

Configure a homeCore broker and follow the service's guide for supported
settings. Use a dedicated broker account scoped to each plugin's MQTT topics.
Supply passwords, tokens and device configuration through private files or
your deployment's secret store.

| Service | Guide |
| --- | --- |
| Teslemetry | [Energy and vehicle telemetry](../services/teslemetry/README.md) |
| Mitsubishi | [CN105 climate integration](../services/mitsubishi/README.md) |
| Electrolux | [Broadlink climate integration](../services/electrolux/README.md) |
| Hisense | [Read-only television integration](../services/hisense/README.md) and [sidecar template](hisense-sidecar.example.yaml) |
| HomeKit | [Package and setup](../services/homekit/README.md) |
| Xiaomi | [Setup service and adapters](../services/xiaomi/README.md) and [extractor template](xiaomi-extractor.knative.example.yaml) |

The Dockerfiles in this directory package release binaries. Build the binary
and place the artifact at the path the corresponding Dockerfile expects
before running a Docker build. For Kubernetes, use the example manifests as
templates and supply image names, network policy, storage and device settings
for your installation.

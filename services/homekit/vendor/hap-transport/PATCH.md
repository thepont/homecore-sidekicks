# Local HomeKit transport patch

This is `hap-transport` 1.3.1 from `phunapps/hap-rust`, source revision
`0193b72d360031b688e4a1999938474c09183893`, Apache-2.0. The local patch adds
an HTTP `Host` header using the connected peer's address to both unencrypted
and encrypted requests. The published release omits it; HAP-NodeJS on a
current Node release rejects pair setup with HTTP 400. The loopback pairing
contract test exercises this against a separate HomeKit accessory implementation.
The discovery patch also retains the interface scope on link-local IPv6
addresses; without it a discovered accessory cannot be reached on the LAN.

Remove the local patch when a pinned upstream release provides the correction
and passes the same contract tests. See `LICENSE` for the upstream license.

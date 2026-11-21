# homeCore Rust SDK snapshot

This is the MIT OR Apache-2.0 SDK from homeCore v0.1.120, revision
`40a9c649e33bd61d8cbb343eebf20b61bd624ca3`. The local SDK change
`ba41001060dc225eab7df05c755d072bd4b735b5` lets a publisher use an existing MQTT
session and preserve opt-in registration retention and heartbeat QoS.
The upstream license files accompany this copy.

The standalone manifest pins the homeCore library dependencies to the same
revision. It keeps rumqttc's optional TLS support out of the local MQTT
connections used by Hisense, HomeKit and Xiaomi.

use if_addrs::{IfAddr, IfOperStatus, Ifv6Addr, Interface};
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

#[test]
fn discovered_link_local_accessory_keeps_its_interface_scope() {
    let ip: Ipv6Addr = "fe80::1234".parse().unwrap();
    let interface = Interface {
        name: "test-lan".into(),
        addr: IfAddr::V6(Ifv6Addr {
            ip,
            netmask: "ffff:ffff:ffff:ffff::".parse().unwrap(),
            prefixlen: 64,
            broadcast: None,
        }),
        index: Some(42),
        oper_status: IfOperStatus::Up,
        is_p2p: false,
    };
    let scoped = mdns_sd::ScopedIp::from(&interface);
    assert_eq!(
        hap_transport::discovery_test_support::socket_address(&scoped, 56605),
        SocketAddr::V6(SocketAddrV6::new(ip, 56605, 0, 42))
    );
}

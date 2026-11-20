//! Bounded Broadlink session for the Electrolux 0x4f9b controller.
use crate::{parse_status, protocol, ClimateStatus};
use packed_struct::prelude::PackedStruct;
use rbroadlink::{
    network::{AuthenticationMessage, CommandMessage},
    traits::CommandTrait,
};
use std::{
    net::{SocketAddr, UdpSocket},
    time::Duration,
};

// Public protocol bootstrap key; per-session keys are negotiated at authentication.
const INITIAL: [u8; 16] = [
    0x09, 0x76, 0x28, 0x34, 0x3f, 0xe9, 0x9e, 0x23, 0x76, 0x5c, 0x15, 0x13, 0xac, 0xcf, 0x8b, 0x02,
];
struct Query;
impl CommandTrait for Query {
    fn packet_type() -> u16 {
        0x6a
    }
}

pub struct Client {
    socket: UdpSocket,
    mac: [u8; 6],
    key: [u8; 16],
    id: u32,
}
impl Client {
    pub fn connect(
        address: SocketAddr,
        mac: [u8; 6],
        timeout: Duration,
    ) -> Result<Self, &'static str> {
        if !address.is_ipv4() || timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err("Invalid device endpoint");
        }
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| "Broadlink socket failed")?;
        socket
            .connect(address)
            .map_err(|_| "Broadlink socket failed")?;
        socket
            .set_read_timeout(Some(timeout))
            .map_err(|_| "Broadlink socket failed")?;
        socket
            .set_write_timeout(Some(timeout))
            .map_err(|_| "Broadlink socket failed")?;
        let mut client = Self {
            socket,
            mac,
            key: INITIAL,
            id: 0,
        };
        let auth = AuthenticationMessage::new("homecore")
            .pack()
            .map_err(|_| "Broadlink authentication failed")?;
        let reply = client.exchange::<AuthenticationMessage>(&auth)?;
        let fields = reply.get(..20).ok_or("Invalid Broadlink authentication")?;
        client.id = u32::from_le_bytes(
            fields[..4]
                .try_into()
                .map_err(|_| "Invalid Broadlink authentication")?,
        );
        client.key.copy_from_slice(&fields[4..20]);
        Ok(client)
    }
    fn exchange<T: CommandTrait>(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let request = CommandMessage::new::<T>(0x4f9b, self.mac, self.id)
            .pack_with_payload(payload, &self.key)
            .map_err(|_| "Invalid Broadlink request")?;
        self.socket
            .send(&request)
            .map_err(|_| "Broadlink request failed")?;
        let mut buf = [0u8; 8192];
        let n = self
            .socket
            .recv(&mut buf)
            .map_err(|_| "Broadlink request timed out")?;
        if n < 0x38
            || n == buf.len()
            || buf[0x28..0x2a] != request[0x28..0x2a]
            || buf[0x22..0x24] != [0, 0]
        {
            return Err("Invalid Broadlink response");
        }
        CommandMessage::unpack_with_payload(buf[..n].to_vec(), &self.key)
            .map_err(|_| "Invalid Broadlink response")
    }
    pub fn status(&mut self) -> Result<ClimateStatus, &'static str> {
        let response = self.exchange::<Query>(&protocol::status_request())?;
        parse_status(&protocol::parse_status_response(&response)?)
    }
}

impl Client {
    pub fn set_power(&mut self, on: bool) -> Result<(), &'static str> {
        let response = self.exchange::<Query>(&protocol::power_request(on)?)?;
        protocol::parse_status_response(&response)?;
        Ok(())
    }
}

impl Client {
    pub fn set_temperature(&mut self, temperature: u8) -> Result<(), &'static str> {
        let response = self.exchange::<Query>(&protocol::temperature_request(temperature)?)?;
        protocol::parse_status_response(&response)?;
        Ok(())
    }
}

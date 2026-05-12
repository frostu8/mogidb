//! Lower-level SRB2 packet management.

use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    mem::size_of,
};

use bytemuck::{Pod, Zeroable};
use derive_more::Display;

use num_enum::{IntoPrimitive, TryFromPrimitive};

const RINGRACERS_VERSION: u8 = 2;
const MAX_PLAYERS: usize = 16;

/// An SRB2 packet.
#[derive(Clone, Debug)]
pub struct Packet {
    checksum: u32,
    payload: Payload,
}

impl Packet {
    /// Unpacks an SRB2 packet.
    pub fn unpack(packet: &[u8]) -> Result<Packet, Error> {
        if packet.len() < 8 {
            return Err(ErrorKind::UnexpectedLength(packet.len()).into());
        }

        let (header, payload) = packet.split_at(8);

        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&header[..4]);
        let checksum = u32::from_le_bytes(checksum);

        let payload_checksum = net_checksum(payload);
        if checksum != payload_checksum {
            return Err(ErrorKind::BadChecksum(payload_checksum).into());
        }

        // Get packet type
        let packet_type = header[6];
        let packet_type =
            PacketType::try_from(packet_type).map_err(ErrorKind::InvalidPacketType)?;

        let payload = match packet_type {
            PacketType::AskInfo => Payload::AskInfo(unpack_payload(payload)?),
            PacketType::ServerInfo => Payload::ServerInfo(unpack_payload(payload)?),
            PacketType::PlayerInfo => Payload::PlayerInfo(unpack_payload(payload)?),
            _ => todo!(),
        };

        Ok(Packet { checksum, payload })
    }

    /// Packs an SRB2 packet.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let inner = self.payload.as_bytes();
        let checksum = net_checksum(inner);
        let len = inner.len() + 8;

        let mut packet = (0..len).map(|_| 0).collect::<Vec<u8>>();

        // Build header
        packet[6] = self.payload.packet_type().into();
        (&mut packet[0..4]).copy_from_slice(&checksum.to_le_bytes());

        (&mut packet[8..]).copy_from_slice(inner);

        Ok(packet)
    }
}

fn unpack_payload<T>(payload: &[u8]) -> Result<T, Error>
where
    T: Pod + Zeroable,
{
    if payload.len() < size_of::<T>() {
        return Err(ErrorKind::UnexpectedLength(payload.len() + 8).into());
    }
    let mut data = T::zeroed();
    bytemuck::bytes_of_mut(&mut data).copy_from_slice(&payload[..size_of::<T>()]);
    Ok(data)
}

/// Packet enumeration.
#[derive(Clone, Debug)]
pub enum Payload {
    AskInfo(AskInfo),
    ServerInfo(ServerInfo),
    PlayerInfo(PlayerInfo),
}

impl Payload {
    /// The packet type.
    pub fn packet_type(&self) -> PacketType {
        match self {
            Payload::AskInfo(_) => PacketType::AskInfo,
            Payload::ServerInfo(_) => PacketType::ServerInfo,
            Payload::PlayerInfo(_) => PacketType::PlayerInfo,
        }
    }

    /// The inner payload as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Payload::AskInfo(pod) => bytemuck::bytes_of(pod),
            Payload::ServerInfo(pod) => bytemuck::bytes_of(pod),
            Payload::PlayerInfo(pod) => bytemuck::bytes_of(pod),
        }
    }
}

/// Asks a server for info.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[repr(packed)]
pub struct AskInfo {
    version: u8,
    time: u32,
}

/// The server info returned in response to [`PacketType::AskInfo`].
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[repr(packed)]
pub struct ServerInfo {
    _255: u8,
    packetversion: u8,
    application: [u8; 16],
    version: u8,
    subversion: u8,
    commit: [u8; 4],
    numberofplayer: u8,
    maxplayer: u8,
    refusereason: u8,
    gametypename: [u8; 24],
    modifiedgame: u8,
    cheatsenabled: u8,
    kartvars: u8,
    fileneedednum: u8,
    time: u32,
    leveltime: u32,
    servername: [u8; 32],
    maptitle: [u8; 33],
    mapmd5: [u8; 16],
    actnum: u8,
    iszone: u8,
    httpsource: [u8; 256],
    avgpwrlv: u16,
}

/// Info about a single player.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[repr(packed)]
pub struct PlayerInfo {
    num: u8,
    name: [u8; 22],
    address: [u8; 4], //wtf, sonicteamjr?
    team: u8,
    skin: u8,
    data: u8, // set to 0xff for compat
    score: i32,
    time_in_server: u16,
}

/// Packet type numbers.
#[derive(Clone, Copy, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum PacketType {
    AskInfo = 12,
    ServerInfo = 13,
    PlayerInfo = 14,
    TellFilesNeeded = 32,
    MoreFilesNeeeded = 33,
}

/// An error serializing or deserializing a packet.
#[derive(Clone, Debug)]
pub struct Error {
    kind: ErrorKind,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.kind)
    }
}

impl<T> From<T> for Error
where
    ErrorKind: From<T>,
{
    fn from(value: T) -> Self {
        Error {
            kind: ErrorKind::from(value),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match &self.kind {
            ErrorKind::InvalidPacketType(err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Display)]
pub enum ErrorKind {
    /// The packet was too short.
    #[display("packet too short, len: {_0}")]
    UnexpectedLength(usize),
    /// A bad checksum was given.
    #[display("bad checksum: {_0}")]
    BadChecksum(u32),
    /// An invalid packet type was given.
    #[display("invalid packet type")]
    InvalidPacketType(num_enum::TryFromPrimitiveError<PacketType>),
}

// Taken from src/d_net.cpp, L:714

/// Calculates the checksum of a packet payload.
fn net_checksum(payload: &[u8]) -> u32 {
    let mut checksum: u32 = 0x1234567;
    for (i, byte) in payload.iter().copied().enumerate() {
        let (a, _) = checksum.overflowing_add(byte as u32);
        checksum = a * (i as u32 + 1);
    }
    checksum
}

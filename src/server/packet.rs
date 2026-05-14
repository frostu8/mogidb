//! Lower-level SRB2 packet management.

use std::{
    convert::Infallible,
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    mem::size_of,
};

use bytemuck::{Pod, Zeroable};
use derive_more::{Display, From};

use mogidb_model::server::{GameSpeed, PlayerInfo, RefuseReason, ServerFlags, ServerInfo};
use num_enum::{IntoPrimitive, TryFromPrimitive};

const RINGRACERS_VERSION: u8 = 2;
const MAX_PLAYERS: usize = 16;

const HEADER_LENGTH: usize = 8;

/// An SRB2 packet.
#[derive(Clone, Debug)]
pub struct Packet {
    /// The inner payload.
    pub payload: Payload,
}

impl Packet {
    /// Creates a new `Packet` for a payload.
    pub fn new(payload: Payload) -> Packet {
        payload.into()
    }

    /// Creates a new [`AskInfo`] packet with a time of zero.
    pub fn ask_info() -> Packet {
        Payload::AskInfo(AskInfo::default()).into()
    }

    /// Unpacks an SRB2 packet.
    pub fn unpack(packet: &[u8]) -> Result<Packet, Error> {
        if packet.len() < 8 {
            return Err(ErrorKind::UnexpectedLength(packet.len()).into());
        }

        let (header, payload) = packet.split_at(HEADER_LENGTH);

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
            PacketType::AskInfo => {
                let payload = unpack_payload::<AskInfoPacked>(payload)?;
                Payload::AskInfo(AskInfo::try_from(payload)?)
            }
            PacketType::ServerInfo => {
                let payload = unpack_payload::<ServerInfoPacked>(payload)?;
                Payload::ServerInfo(ServerInfo::try_from(payload)?)
            }
            PacketType::PlayerInfo => {
                let payload = unpack_payload::<PlayerInfoPacked>(payload)?;
                Payload::PlayerInfo(PlayerInfo::try_from(payload)?)
            }
            _ => todo!(),
        };

        Ok(Packet { payload })
    }

    /// Packs an SRB2 packet.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let len = self.payload.len() + HEADER_LENGTH;
        let mut packet = (0..len).map(|_| 0).collect::<Vec<u8>>();

        // Write inner payload
        self.payload.to_bytes(&mut packet[HEADER_LENGTH..]);
        // calculate checksum
        let checksum = net_checksum(&packet[HEADER_LENGTH..]);

        // Build header
        packet[6] = self.payload.packet_type().into();
        (&mut packet[0..4]).copy_from_slice(&checksum.to_le_bytes());

        Ok(packet)
    }
}

impl From<Payload> for Packet {
    fn from(value: Payload) -> Self {
        Packet { payload: value }
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

    /// The len of the packet.
    pub fn len(&self) -> usize {
        match self {
            Payload::AskInfo(_) => size_of::<AskInfoPacked>(),
            _ => todo!(),
        }
    }

    /// Writes the inner payload as bytes.
    ///
    /// # Panics
    /// Panics if the buffer is not large enough to store the payload. Use
    /// [`Payload::len`] to ensure this.
    pub fn to_bytes(&self, buf: &mut [u8]) {
        let len = self.len();
        assert!(len <= buf.len());
        let buf = &mut buf[..len];

        match self {
            Payload::AskInfo(pod) => {
                let payload = AskInfoPacked {
                    version: RINGRACERS_VERSION,
                    time: pod.time,
                };
                buf.copy_from_slice(bytemuck::bytes_of(&payload));
            }
            _ => todo!(),
        }
    }
}

/// Asks a server for info.
#[derive(Clone, Debug)]
pub struct AskInfo {
    version: u8,
    time: u32,
}

impl Default for AskInfo {
    fn default() -> Self {
        AskInfo {
            version: RINGRACERS_VERSION,
            time: 0,
        }
    }
}

/// Asks a server for info.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[repr(packed)]
struct AskInfoPacked {
    version: u8,
    time: u32,
}

impl From<AskInfoPacked> for AskInfo {
    fn from(value: AskInfoPacked) -> Self {
        AskInfo {
            version: value.version,
            time: value.time,
        }
    }
}

/// The server info returned in response to [`PacketType::AskInfo`].
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[repr(packed)]
struct ServerInfoPacked {
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

impl TryFrom<ServerInfoPacked> for ServerInfo {
    type Error = Error;

    fn try_from(value: ServerInfoPacked) -> Result<Self, Self::Error> {
        // First, we gotta convert all these stupid dumb strings
        let application = cstr(&value.application)?;
        let gametype_name = cstr(&value.gametypename)?;
        let server_name = cstr(&value.servername)?;
        let map_title = cstr(&value.maptitle)?;
        let http_source = cstr(&value.httpsource)?;

        // Calculate MD5 of map, and commit hash
        let map_md5 = base16::encode_lower(&value.mapmd5);
        let commit = base16::encode_lower(&value.commit);

        // Unpack kartvars
        let game_speed = GameSpeed::try_from(value.kartvars & 0x03)?;

        let flags_inner = value.kartvars as u32 & ServerFlags::all().bits();
        let flags = ServerFlags::from_bits(flags_inner)
            .ok_or_else(|| ErrorKind::InvalidFlags(flags_inner))?;

        // Unpack refuse reason
        let refuse_reason = RefuseReason::try_from(value.refusereason)?;

        Ok(ServerInfo {
            application: application.to_owned(),
            version: value.version,
            subversion: value.subversion,
            commit,
            gametype_name: gametype_name.to_owned(),
            server_name: server_name.to_owned(),
            number_of_players: value.numberofplayer,
            max_players: value.maxplayer,
            modified_game: value.modifiedgame != 0,
            cheats_enabled: value.cheatsenabled != 0,
            avg_mobiums: value.avgpwrlv,
            game_speed,
            flags,
            refuse_reason,
            time: value.time,
            level_time: value.leveltime,
            map_title: map_title.to_owned(),
            map_md5,
            actnum: value.actnum,
            is_zone: value.iszone != 0,
            number_of_files: value.fileneedednum,
            http_source: http_source.to_owned(),
        })
    }
}

/// Info about a single player.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[repr(packed)]
struct PlayerInfoPacked {
    num: u8,
    name: [u8; 22],
    address: [u8; 4], //wtf, sonicteamjr?
    team: u8,
    skin: u8,
    data: u8, // set to 0xff for compat
    score: i32,
    timeinserver: u16,
}

impl TryFrom<PlayerInfoPacked> for PlayerInfo {
    type Error = Error;

    fn try_from(value: PlayerInfoPacked) -> Result<Self, Self::Error> {
        // Do strings
        let name = cstr(&value.name)?;

        // We intentionally ignore value.address for obvious reasons

        Ok(PlayerInfo {
            num: value.num,
            name: name.to_owned(),
            team: value.team,
            score: value.score,
            time_in_server: value.timeinserver,
        })
    }
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

impl From<Infallible> for Error {
    fn from(_value: Infallible) -> Self {
        unreachable!()
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

#[derive(Clone, Debug, Display, From)]
pub enum ErrorKind {
    /// The packet was too short.
    #[display("packet too short, len: {_0}")]
    #[from(ignore)]
    UnexpectedLength(usize),
    /// A bad checksum was given.
    #[display("invalid checksum: {_0}")]
    #[from(ignore)]
    BadChecksum(u32),
    /// Bad kart vars.
    #[display("unexpected flags: {_0}")]
    #[from(ignore)]
    InvalidFlags(u32),
    #[display("unknown game speed")]
    InvalidGameSpeed(num_enum::TryFromPrimitiveError<GameSpeed>),
    #[display("unknown refuses reason")]
    InvalidRefuseReason(num_enum::TryFromPrimitiveError<RefuseReason>),
    /// A UTF8 error occured when decoding.
    #[display("invalid utf8")]
    Utf8(std::str::Utf8Error),
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

/// Gets all characters before a nul terminator, and returns them as a str.
fn cstr(bytes: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let bytes = match bytes.iter().position(|&b| b == 0x00) {
        Some(idx) => &bytes[..idx],
        None => bytes,
    };
    str::from_utf8(bytes)
}

//! Server management and knocking.

pub mod packet;

use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    io,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
    time::{Duration, Instant},
};

use derive_more::From;

use mogidb_model::server::{PlayerInfo, ServerInfo};

use tokio::{net::UdpSocket, time::timeout};

use packet::{Packet, Payload};

/// Server tracker.
///
/// Cheaply cloneable.
#[derive(Clone, Debug)]
pub struct ServerTracker {
    state: Arc<ServerTrackerState>,
}

#[derive(Debug)]
struct ServerTrackerState {
    servers: papaya::HashMap<SocketAddr, Entry>,
    max_ping_count: usize,
    ratelimit_duration: Duration,
    timeout: Duration,
}

impl ServerTracker {
    /// Creates a new `ServerTracker`.
    pub fn new() -> ServerTracker {
        ServerTracker::default()
    }

    /// Knocks a server.
    pub async fn knock(
        &self,
        ip: impl ToSocketAddrs,
    ) -> Result<(ServerInfo, Vec<PlayerInfo>), Error> {
        // Get address
        let address = ip.to_socket_addrs()?.next().expect("at least one address");

        let now = Instant::now();

        // Resolve entry
        let servers = self.state.servers.pin();
        if let Some(server) = servers.get(&address) {
            // Check if we should try to ping or if we've pelted the server enough
            // for now.
            if now < server.last_ping + self.state.ratelimit_duration {
                // Return last result
                return Ok((server.info.clone(), server.players.clone()));
            }
        }
        drop(servers);

        // Begin resolving knock
        let socket = UdpSocket::bind(address).await?;
        match timeout(self.state.timeout, get_info(address, &socket)).await {
            Ok(Ok(result)) => {
                // Set entry
                let servers = self.state.servers.pin();

                let mut pings = if let Some(entry) = servers.get(&address) {
                    if entry.pings.len() >= 8 {
                        // Truncate
                        entry.pings.iter().copied().skip(1).collect::<Vec<_>>()
                    } else {
                        entry.pings.clone()
                    }
                } else {
                    // Start new pings list
                    Vec::new()
                };
                pings.push(result.ping);

                servers.insert(
                    address,
                    Entry {
                        socket_addr: address,
                        last_ping: now,
                        pings,
                        info: result.info.clone(),
                        players: result.players.clone(),
                    },
                );

                Ok((result.info, result.players))
            }
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err(Error::Timeout(self.state.timeout)),
        }
    }
}

impl Default for ServerTracker {
    fn default() -> Self {
        ServerTracker {
            state: Arc::new(ServerTrackerState {
                servers: papaya::HashMap::new(),
                max_ping_count: 8,
                ratelimit_duration: Duration::from_secs(30),
                timeout: Duration::from_secs(2),
            }),
        }
    }
}

/// Sends an ask packet, and times the first response
async fn ask(socket: &UdpSocket, buf: &mut [u8]) -> Result<Duration, Error> {
    // Create an ask packet
    let packet = Packet::ask_info();
    let data = packet.pack()?;

    tracing::debug!("sending ask packet");

    let start_time = Instant::now();
    socket.send(&data).await?;

    // Wait for a response
    socket.recv(buf).await?;
    let end_time = Instant::now();

    Ok(end_time - start_time)
}

async fn get_info(remote: SocketAddr, socket: &UdpSocket) -> Result<GetInfoResult, Error> {
    // Start collecting data
    let mut info = None::<ServerInfo>;
    let mut players = Vec::<PlayerInfo>::new();

    // Send the ask packet to kick things off
    let mut buf = [0u8; 1500];
    let ping = ask(socket, &mut buf).await?;

    loop {
        // Decode packet
        let packet = match Packet::unpack(&buf) {
            Ok(packet) => packet,
            Err(err) => {
                tracing::warn!("got error {} knocking for server {}", err, remote);
                continue;
            }
        };

        match packet.payload {
            Payload::ServerInfo(recv_info) => info = Some(recv_info),
            Payload::PlayerInfo(recv_player) if !recv_player.is_empty() => {
                players.push(recv_player);
            }
            Payload::PlayerInfo(_) => (),
            _ => tracing::warn!(
                "got unexpected packet {:?} knocking for server {}",
                packet.payload.packet_type(),
                remote
            ),
        }

        // Check if we have all the pieces
        if let Some(info_ref) = info.as_ref() {
            if players.len() >= info_ref.number_of_players as usize {
                let info = info.unwrap();
                return Ok(GetInfoResult {
                    ping,
                    info,
                    players,
                });
            }
        }

        // Get next packet
        socket.recv(&mut buf).await?;
    }
}

struct GetInfoResult {
    info: ServerInfo,
    players: Vec<PlayerInfo>,
    ping: Duration,
}

/// An error for [`ServerTracker`].
#[derive(Debug, From)]
pub enum Error {
    /// The handshake timed out.
    #[from(ignore)]
    Timeout(Duration),
    /// An IO error occured
    Io(io::Error),
    /// An error occured during the handshake process.
    Packet(packet::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Timeout(time) => write!(f, "timeout occured after {}s", time.as_secs()),
            Error::Io(_err) => write!(f, "IO error occured"),
            Error::Packet(_err) => write!(f, "server failed handshake"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Packet(err) => Some(err),
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    socket_addr: SocketAddr,
    last_ping: Instant,
    pings: Vec<Duration>,

    info: ServerInfo,
    players: Vec<PlayerInfo>,
}

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

use chrono::{DateTime, Utc};
use derive_more::From;

use tokio::{
    net::UdpSocket,
    sync::{Semaphore, TryAcquireError},
    time::timeout,
};

use packet::{Packet, Payload, PlayerInfo, ServerInfo};

/// Server tracker.
#[derive(Debug)]
pub struct ServerTracker {
    state: ServerTrackerState,
}

#[derive(Debug)]
struct ServerTrackerState {
    servers: papaya::HashMap<SocketAddr, Entry>,
    locks: papaya::HashMap<SocketAddr, Arc<Semaphore>>,

    max_ping_count: usize,
    ratelimit_duration: Duration,
    timeout: Duration,
}

impl ServerTracker {
    /// Creates a new `ServerTracker`.
    pub fn new() -> ServerTracker {
        ServerTracker::default()
    }

    /// Gets cached information about a server.
    pub fn get(&self, ip: impl ToSocketAddrs) -> Option<KnockResult> {
        // Get address
        let address = ip
            .to_socket_addrs()
            .ok()?
            .next()
            .expect("at least one address");

        let servers = self.state.servers.pin();
        if let Some(server) = servers.get(&address) {
            // Return last result
            Some(KnockResult {
                info: server.info.clone(),
                players: server.players.clone(),
                last_ping_time: server.last_ping_time,
            })
        } else {
            None
        }
    }

    /// Knocks a server.
    ///
    /// This will first try to pull from the cache.
    pub async fn knock(&self, ip: impl ToSocketAddrs) -> Result<KnockResult, Error> {
        // Get address
        let address = ip.to_socket_addrs()?.next().expect("at least one address");

        let now = Instant::now();
        let now_utc = Utc::now();

        // Resolve entry
        let mut pings = {
            let servers = self.state.servers.pin();
            if let Some(server) = servers.get(&address) {
                // Check if we should try to ping or if we've pelted the server enough
                // for now.
                if now < server.last_ping + self.state.ratelimit_duration {
                    // Return last result
                    return Ok(KnockResult {
                        info: server.info.clone(),
                        players: server.players.clone(),
                        last_ping_time: server.last_ping_time,
                    });
                } else {
                    server.pings.clone()
                }
            } else {
                Vec::new()
            }
        };

        // We need to fetch fresh data, try locking first
        let lock = {
            let locks_pin = self.state.locks.pin();
            Arc::clone(locks_pin.get_or_insert_with(address, || Arc::new(Semaphore::new(1))))
        };

        let _permit = match lock.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                // Another taskis already knocking for us, wait for them to be done.
                let permit = lock.acquire().await;
                drop(permit);

                // Get data
                let servers_pin = self.state.servers.pin();
                let server = servers_pin
                    .get(&address)
                    .expect("fresh or stale server info");

                return Ok(KnockResult {
                    info: server.info.clone(),
                    players: server.players.clone(),
                    last_ping_time: server.last_ping_time,
                });
            }
            Err(_) => panic!("semaphore poisoned"),
        };

        let port = address.port();

        // Begin resolving knock
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
        socket.connect(address).await?;
        match timeout(self.state.timeout, get_info(address, &socket)).await {
            Ok(Ok(result)) => {
                // Set entry
                let servers = self.state.servers.pin();

                if pings.len() >= self.state.max_ping_count {
                    // Truncate
                    pings.rotate_left(1);
                    pings.pop();
                }
                pings.push(result.ping);

                servers.insert(
                    address,
                    Entry {
                        socket_addr: address,
                        last_ping: now,
                        last_ping_time: now_utc,
                        pings,
                        info: result.info.clone(),
                        players: result.players.clone(),
                    },
                );

                Ok(KnockResult {
                    info: result.info,
                    players: result.players,
                    last_ping_time: now_utc,
                })
            }
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err(Error::Timeout(self.state.timeout)),
        }
    }
}

impl Default for ServerTracker {
    fn default() -> Self {
        ServerTracker {
            state: ServerTrackerState {
                servers: papaya::HashMap::new(),
                locks: papaya::HashMap::new(),
                max_ping_count: 8,
                ratelimit_duration: Duration::from_secs(30),
                timeout: Duration::from_secs(2),
            },
        }
    }
}

/// The result of a server knock.
#[derive(Clone, Debug)]
pub struct KnockResult {
    pub info: ServerInfo,
    pub players: Vec<PlayerInfo>,
    pub last_ping_time: DateTime<Utc>,
}

/// Sends an ask packet, and times the first response
async fn ask(socket: &UdpSocket, buf: &mut [u8]) -> Result<(usize, Duration), Error> {
    // Create an ask packet
    let packet = Packet::ask_info();
    let data = packet.pack()?;

    tracing::debug!("sending ask packet; {:?}", data);

    let start_time = Instant::now();
    socket.send(&data).await?;

    // Wait for a response
    let len = socket.recv(buf).await?;
    let end_time = Instant::now();

    Ok((len, end_time - start_time))
}

async fn get_info(remote: SocketAddr, socket: &UdpSocket) -> Result<GetInfoResult, Error> {
    // Start collecting data
    let mut info = None::<ServerInfo>;
    let mut players = Vec::<PlayerInfo>::new();

    // Send the ask packet to kick things off
    let mut buf = [0u8; 1500];
    let (mut packet_len, ping) = ask(socket, &mut buf).await?;

    loop {
        // Decode packet
        let packet = match Packet::unpack(&buf[..packet_len]) {
            Ok(packet) => packet,
            Err(err) => {
                tracing::warn!("got error knocking for server {}", remote);
                tracing::warn!("{:?}", eyre::Report::new(err));
                continue;
            }
        };

        tracing::debug!(
            "got packet from remote {}: {:?}",
            remote,
            packet.packet_type()
        );

        match packet.payload {
            Payload::ServerInfo(recv_info) => {
                tracing::debug!("expecting {} players", recv_info.number_of_players);
                info = Some(recv_info);
            }
            Payload::PlayerInfo(recv_players) => {
                players.extend(recv_players.into_iter().filter(|p| !p.is_empty()));
            }
            _ => tracing::warn!(
                "got unexpected packet {:?} knocking for server {}",
                packet.packet_type(),
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
        packet_len = socket.recv(&mut buf).await?;
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
    #[allow(dead_code)]
    socket_addr: SocketAddr,
    last_ping: Instant,
    last_ping_time: DateTime<Utc>,
    pings: Vec<Duration>,

    info: ServerInfo,
    players: Vec<PlayerInfo>,
}

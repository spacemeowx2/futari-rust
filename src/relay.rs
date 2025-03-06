use crate::logging::log_utils::{error, info, warn};
use crate::types::{commands, current_time_millis, protocols, Msg};

use std::collections::{HashMap, HashSet};
use std::io::{Error as IoError, Result as IoResult};
use std::net::SocketAddr;
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Receiver, Sender};

// Constants
const MAX_STREAMS: usize = 10;
const VERSION: &str = "Futari-Rust-Relay v0.1.0";

/// Type alias for ClientMap
type ClientMap = DashMap<String, Arc<ActiveClient>>;
/// Type alias for AddressMap
type AddressMap = DashMap<u32, String>;

/// Message to be sent to a client
#[derive(Debug)]
pub struct ClientMessage {
    pub client_key: String,
    pub message: String,
}

/// Represents an active client connection
struct ActiveClient {
    client_key: String,
    #[allow(dead_code)]
    addr: SocketAddr,
    tcp_streams: RwLock<HashMap<u32, u32>>,
    pending_streams: RwLock<HashSet<u32>>,
    tx: Sender<String>,
    last_heartbeat: RwLock<i64>,
}

impl ActiveClient {
    fn new(client_key: String, addr: SocketAddr, tx: Sender<String>) -> Self {
        Self {
            client_key,
            addr,
            tcp_streams: RwLock::new(HashMap::new()),
            pending_streams: RwLock::new(HashSet::new()),
            tx,
            last_heartbeat: RwLock::new(current_time_millis()),
        }
    }

    /// Add a pending TCP stream
    fn add_pending_stream(&self, stream_id: u32) -> bool {
        let mut pending = self.pending_streams.write();
        if pending.len() >= MAX_STREAMS {
            return false;
        }
        pending.insert(stream_id);
        true
    }

    /// Accept a TCP stream, moving it from pending to active
    fn accept_stream(&self, stream_id: u32, dest_ip: u32) -> bool {
        let mut pending = self.pending_streams.write();
        if !pending.remove(&stream_id) {
            return false;
        }

        let mut streams = self.tcp_streams.write();
        streams.insert(stream_id, dest_ip);
        true
    }

    /// Close a TCP stream
    fn close_stream(&self, stream_id: u32) -> bool {
        let mut streams = self.tcp_streams.write();
        streams.remove(&stream_id).is_some()
    }

    /// Update the last heartbeat time
    fn update_heartbeat(&self) {
        let mut last = self.last_heartbeat.write();
        *last = current_time_millis();
    }

    /// Check if the client is still alive
    fn is_alive(&self, timeout: i64) -> bool {
        let last = *self.last_heartbeat.read();
        (current_time_millis() - last) < timeout
    }
}

/// The FutariRelay server
#[derive(Clone)]
pub struct FutariRelay {
    clients: Arc<ClientMap>,
    ip_to_client: Arc<AddressMap>,
    message_tx: Sender<ClientMessage>,
}

impl FutariRelay {
    /// Create a new FutariRelay server
    pub fn new() -> (Self, Receiver<ClientMessage>) {
        let clients = Arc::new(DashMap::new());
        let ip_to_client = Arc::new(DashMap::new());
        let (message_tx, message_rx) = mpsc::channel(1000);

        (
            Self {
                clients,
                ip_to_client,
                message_tx,
            },
            message_rx,
        )
    }

    /// Start the relay server
    pub async fn start(&self, addr: &str) -> IoResult<()> {
        info("Relay", &format!("Starting FutariRelay on {}", addr));
        let listener = TcpListener::bind(addr).await?;

        // Start a background task to handle outgoing messages
        self.spawn_client_cleanup_task();

        // Accept and handle connections
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    info("Relay", &format!("New connection from {}", addr));
                    let clients = self.clients.clone();
                    let ip_to_client = self.ip_to_client.clone();
                    let message_tx = self.message_tx.clone();

                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_client(socket, addr, clients, ip_to_client, message_tx)
                                .await
                        {
                            error("Relay", &format!("Error handling client {}: {}", addr, e));
                        }
                    });
                }
                Err(e) => {
                    error("Relay", &format!("Accept error: {}", e));
                }
            }
        }
    }

    /// Spawns a task that periodically checks for inactive clients and removes them
    fn spawn_client_cleanup_task(&self) {
        let clients = self.clients.clone();
        let ip_to_client = self.ip_to_client.clone();

        tokio::spawn(async move {
            let timeout = 30_000; // 30 seconds timeout
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

                let mut keys_to_remove = Vec::new();
                for entry in clients.iter() {
                    let (key, client) = (entry.key().clone(), entry.value().clone());
                    if !client.is_alive(timeout) {
                        keys_to_remove.push(key);
                    }
                }

                for key in keys_to_remove {
                    info("Relay", &format!("Removing inactive client {}", key));
                    clients.remove(&key);
                    // Clean up IP mapping
                    for entry in ip_to_client.iter() {
                        if entry.value() == &key {
                            ip_to_client.remove(entry.key());
                        }
                    }
                }
            }
        });
    }

    /// Process and relay messages to other clients
    pub async fn process_messages(&self, mut rx: Receiver<ClientMessage>) {
        while let Some(msg) = rx.recv().await {
            if let Some(client) = self.clients.get(&msg.client_key) {
                if let Err(e) = client.tx.send(msg.message).await {
                    error(
                        "Relay",
                        &format!("Failed to send message to {}: {}", msg.client_key, e),
                    );
                }
            }
        }
    }

    /// Handle a client connection
    async fn handle_client(
        socket: TcpStream,
        addr: SocketAddr,
        clients: Arc<ClientMap>,
        ip_to_client: Arc<AddressMap>,
        message_tx: Sender<ClientMessage>,
    ) -> IoResult<()> {
        // Split the socket into a reader and writer
        let (read_half, mut write_half) = socket.into_split();
        let mut reader = BufReader::new(read_half);

        // Create a channel for sending messages to the client
        let (tx, mut rx) = mpsc::channel(100);

        // Read the first message which should be a CTL_START
        let mut buf = String::new();
        reader.read_line(&mut buf).await?;

        let msg = match Msg::parse(&buf) {
            Some(msg) if msg.cmd == commands::CTL_START => msg,
            _ => {
                error(
                    "Relay",
                    &format!("First message from {} is not CTL_START", addr),
                );
                return Err(IoError::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid first message",
                ));
            }
        };

        // Extract the client key from the message data
        let client_key = match &msg.data {
            Some(key) if !key.is_empty() => key.clone(),
            _ => {
                error("Relay", "CTL_START message missing client key");
                return Err(IoError::new(
                    std::io::ErrorKind::InvalidData,
                    "Missing client key",
                ));
            }
        };

        info(
            "Relay",
            &format!("Client {} registered with key {}", addr, client_key),
        );

        // Create an active client
        let client = Arc::new(ActiveClient::new(client_key.clone(), addr, tx.clone()));

        // Store the client
        clients.insert(client_key.clone(), client.clone());

        // Register the client's IP
        if let Some(src_ip) = msg.src {
            ip_to_client.insert(src_ip, client_key.clone());
        }

        // Send version information back
        let mut response = Msg::new(commands::CTL_START);
        response.data = Some(VERSION.to_string());
        write_half
            .write_all(response.to_string().as_bytes())
            .await?;
        write_half.flush().await?;

        // Spawn a task to forward messages to the client
        let write_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                    error("Relay", &format!("Write error: {}", e));
                    break;
                }
                if let Err(e) = write_half.flush().await {
                    error("Relay", &format!("Flush error: {}", e));
                    break;
                }
            }
        });

        // Read and process messages from the client
        loop {
            buf.clear();
            if reader.read_line(&mut buf).await? == 0 {
                // Connection closed
                break;
            }

            if let Some(msg) = Msg::parse(&buf) {
                Self::process_message(&msg, &client, &clients, &ip_to_client, &message_tx).await?;
            } else {
                warn("Relay", &format!("Invalid message from {}: {}", addr, buf));
            }
        }

        // Clean up when the connection closes
        clients.remove(&client_key);
        write_task.abort();

        info("Relay", &format!("Client {} disconnected", addr));
        Ok(())
    }

    /// Process a message received from a client
    async fn process_message(
        msg: &Msg,
        client: &Arc<ActiveClient>,
        clients: &Arc<ClientMap>,
        ip_to_client: &Arc<AddressMap>,
        message_tx: &Sender<ClientMessage>,
    ) -> IoResult<()> {
        // Update heartbeat time
        client.update_heartbeat();

        match msg.cmd {
            commands::CTL_HEARTBEAT => {
                // Just update the heartbeat time which was already done above
                // No response needed
            }

            commands::CTL_TCP_CONNECT => {
                // Handle TCP stream connection request
                if let Some(sid) = msg.sid {
                    if client.add_pending_stream(sid) {
                        // Added to pending streams successfully
                    } else {
                        warn(
                            "Relay",
                            &format!("Client {} has too many pending streams", client.client_key),
                        );
                    }
                }
            }

            commands::CTL_TCP_ACCEPT => {
                // Handle TCP stream acceptance
                if let (Some(sid), Some(src_ip)) = (msg.sid, msg.src) {
                    if client.accept_stream(sid, src_ip) {
                        // Send accept acknowledgement
                        let mut response = Msg::new(commands::CTL_TCP_ACCEPT_ACK);
                        response.sid = Some(sid);
                        message_tx
                            .send(ClientMessage {
                                client_key: client.client_key.clone(),
                                message: response.to_string(),
                            })
                            .await
                            .map_err(|e| IoError::new(std::io::ErrorKind::Other, e.to_string()))?;
                    }
                }
            }

            commands::CTL_TCP_CLOSE => {
                // Handle TCP stream closure
                if let Some(sid) = msg.sid {
                    client.close_stream(sid);
                }
            }

            commands::DATA_SEND => {
                // Forward data to the destination client
                if let Some(dst_ip) = msg.dst {
                    if let Some(dst_key) = ip_to_client.get(&dst_ip) {
                        message_tx
                            .send(ClientMessage {
                                client_key: dst_key.clone(),
                                message: msg.to_string(),
                            })
                            .await
                            .map_err(|e| IoError::new(std::io::ErrorKind::Other, e.to_string()))?;
                    }
                }
            }

            commands::DATA_BROADCAST => {
                // Broadcast to all clients except the sender
                if msg.proto == Some(protocols::UDP) {
                    for entry in clients.iter() {
                        let dst_key = entry.key();
                        if *dst_key != client.client_key {
                            message_tx
                                .send(ClientMessage {
                                    client_key: dst_key.clone(),
                                    message: msg.to_string(),
                                })
                                .await
                                .map_err(|e| {
                                    IoError::new(std::io::ErrorKind::Other, e.to_string())
                                })?;
                        }
                    }
                }
            }

            _ => {
                // Unhandled command
                warn("Relay", &format!("Unhandled command: {}", msg.cmd));
            }
        }

        Ok(())
    }
}

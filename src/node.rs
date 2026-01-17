//! Reticulum Node abstraction for testable networking.
//!
//! This module provides a `Node` struct that encapsulates a complete Reticulum
//! node with transport, identity, destination, and event processing. Multiple
//! nodes can be instantiated in tests to verify end-to-end communication.
//!
//! # Example
//!
//! ```ignore
//! let node_a = Node::new("node_a", "dublin.connect.reticulum.network:4965").await?;
//! let node_b = Node::new("node_b", "dublin.connect.reticulum.network:4965").await?;
//!
//! // Both nodes announce
//! node_a.announce().await;
//! node_b.announce().await;
//!
//! // Node A waits for Node B's announce and sends a message
//! let dest_b = node_a.wait_for_announce(node_b.address_hash(), timeout).await?;
//! node_a.send_message(dest_b, b"Hello!").await?;
//!
//! // Node B receives the message
//! let (from, data) = node_b.recv_message(timeout).await?;
//! ```

use log::{debug, warn};
use rand_core::OsRng;
use reticulum::destination::link::{Link, LinkEvent, LinkStatus};
use reticulum::destination::{DestinationDesc, DestinationName, SingleInputDestination};
use reticulum::hash::AddressHash;
use reticulum::identity::PrivateIdentity;
use reticulum::iface::tcp_client::TcpClient;
use reticulum::transport::{Transport, TransportConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// Type alias for the link map to reduce type complexity.
type LinkMap = Arc<Mutex<HashMap<AddressHash, Arc<Mutex<Link>>>>>;

/// Type alias for the destination map.
type DestinationMap = Arc<Mutex<HashMap<AddressHash, DestinationDesc>>>;

/// Error type for Node operations.
#[derive(Debug)]
pub enum NodeError {
    /// Timeout waiting for an operation.
    Timeout,
    /// Link was closed before completion.
    LinkClosed,
    /// Failed to create data packet.
    PacketError(String),
    /// Channel was closed.
    ChannelClosed,
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeError::Timeout => write!(f, "operation timed out"),
            NodeError::LinkClosed => write!(f, "link closed"),
            NodeError::PacketError(e) => write!(f, "packet error: {}", e),
            NodeError::ChannelClosed => write!(f, "channel closed"),
        }
    }
}

impl std::error::Error for NodeError {}

/// Incoming message from the network.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// Source address hash (link ID).
    pub from: AddressHash,
    /// Message payload.
    pub data: Vec<u8>,
}

/// Link activation event - signals when a link becomes active or closes.
#[derive(Debug, Clone)]
pub enum LinkActivationEvent {
    /// Link became active.
    Activated(AddressHash),
    /// Link was closed.
    Closed(AddressHash),
}

/// A Reticulum node that handles its own event processing.
///
/// Each node runs a background task that processes announces, link events,
/// and incoming messages. This allows multiple nodes to operate independently
/// in the same process.
pub struct Node {
    /// The underlying transport.
    transport: Arc<Mutex<Transport>>,
    /// Our destination for receiving messages.
    destination: Arc<Mutex<SingleInputDestination>>,
    /// Our address hash.
    address_hash: AddressHash,
    /// Active links by destination hash.
    links: LinkMap,
    /// Known destination descriptors (from announces).
    known_destinations: DestinationMap,
    /// Channel for incoming messages.
    message_tx: broadcast::Sender<IncomingMessage>,
    /// Channel for announce notifications.
    announce_tx: broadcast::Sender<AddressHash>,
    /// Channel for link activation events.
    link_activation_tx: broadcast::Sender<LinkActivationEvent>,
    /// Cancellation token for shutdown.
    cancel: CancellationToken,
    /// Background task handle.
    _task: tokio::task::JoinHandle<()>,
}

impl Node {
    /// Create a new node and connect to the specified testnet server.
    ///
    /// The node will:
    /// 1. Generate a new random identity
    /// 2. Create a transport and connect to the testnet
    /// 3. Create a destination for the given name
    /// 4. Start a background task for event processing
    pub async fn new(dest_name: &str, testnet_server: &str) -> Self {
        let identity = PrivateIdentity::new_from_rand(OsRng);
        // Use unique transport name for logging
        let config = TransportConfig::new(dest_name, &identity, false);
        let transport = Arc::new(Mutex::new(Transport::new(config)));

        // Connect to testnet
        {
            let t = transport.lock().await;
            t.iface_manager()
                .lock()
                .await
                .spawn(TcpClient::new(testnet_server), TcpClient::spawn);
        }

        // Wait for interface to initialize
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Create destination
        let dest_name_obj = DestinationName::new("integration_test", dest_name);
        let destination = {
            let mut t = transport.lock().await;
            t.add_destination(identity, dest_name_obj).await
        };

        // Get our address hash
        let address_hash = {
            let dest = destination.lock().await;
            dest.desc.address_hash
        };

        // Create channels
        let (message_tx, _) = broadcast::channel(100);
        let (announce_tx, _) = broadcast::channel(100);
        let (link_activation_tx, _) = broadcast::channel(100);

        // Create shared state
        let links = Arc::new(Mutex::new(HashMap::new()));
        let known_destinations = Arc::new(Mutex::new(HashMap::new()));

        // Spawn background event processing task
        let cancel = CancellationToken::new();
        let task = Self::spawn_event_task(
            transport.clone(),
            links.clone(),
            known_destinations.clone(),
            message_tx.clone(),
            announce_tx.clone(),
            link_activation_tx.clone(),
            cancel.clone(),
        )
        .await;

        Self {
            transport,
            destination,
            address_hash,
            links,
            known_destinations,
            message_tx,
            announce_tx,
            link_activation_tx,
            cancel,
            _task: task,
        }
    }

    /// Get this node's address hash.
    pub fn address_hash(&self) -> AddressHash {
        self.address_hash
    }

    /// Announce this node's presence to the network.
    pub async fn announce(&self) {
        let t = self.transport.lock().await;
        t.send_announce(&self.destination, None).await;
        debug!("Node {} announced", format_hash_static(&self.address_hash));
    }

    /// Wait for an announce from a specific address hash.
    ///
    /// Returns the destination descriptor when found.
    pub async fn wait_for_announce(
        &self,
        target: AddressHash,
        timeout_duration: Duration,
    ) -> Result<DestinationDesc, NodeError> {
        // Subscribe FIRST to avoid missing announces that arrive between check and subscribe
        let mut rx = self.announce_tx.subscribe();

        // Then check if we already know this destination
        {
            let known = self.known_destinations.lock().await;
            if let Some(desc) = known.get(&target) {
                return Ok(*desc);
            }
        }

        // Wait for the announce
        timeout(timeout_duration, async {
            loop {
                match rx.recv().await {
                    Ok(hash) if hash == target => {
                        let known = self.known_destinations.lock().await;
                        if let Some(desc) = known.get(&target) {
                            return Ok(*desc);
                        }
                    }
                    Ok(_) => continue,
                    Err(_) => return Err(NodeError::ChannelClosed),
                }
            }
        })
        .await
        .map_err(|_| NodeError::Timeout)?
    }

    /// Create a link to a destination and wait for it to become active.
    pub async fn create_link(
        &self,
        dest: DestinationDesc,
        timeout_duration: Duration,
    ) -> Result<(), NodeError> {
        let dest_hash = dest.address_hash;

        // Check if link already exists and is active
        {
            let links = self.links.lock().await;
            if let Some(link) = links.get(&dest_hash) {
                let link_guard = link.lock().await;
                if link_guard.status() == LinkStatus::Active {
                    return Ok(());
                }
            }
        }

        // Subscribe to link activation events BEFORE creating the link
        // to avoid missing the activation event
        let mut rx = self.link_activation_tx.subscribe();

        // Create new link
        let link = {
            let t = self.transport.lock().await;
            t.link(dest).await
        };

        // Store the link
        {
            let mut links = self.links.lock().await;
            links.insert(dest_hash, link);
        }

        // Wait for link activation event
        timeout(timeout_duration, async {
            loop {
                match rx.recv().await {
                    Ok(LinkActivationEvent::Activated(hash)) => {
                        if hash == dest_hash {
                            return Ok(());
                        }
                    }
                    Ok(LinkActivationEvent::Closed(hash)) => {
                        if hash == dest_hash {
                            return Err(NodeError::LinkClosed);
                        }
                    }
                    Err(_) => return Err(NodeError::ChannelClosed),
                }
            }
        })
        .await
        .map_err(|_| NodeError::Timeout)?
    }

    /// Send a message to a destination.
    ///
    /// The link must already be established (call `create_link` first).
    pub async fn send_message(&self, dest_hash: AddressHash, data: &[u8]) -> Result<(), NodeError> {
        let link = {
            let links = self.links.lock().await;
            links
                .get(&dest_hash)
                .cloned()
                .ok_or(NodeError::LinkClosed)?
        };

        let packet = {
            let link_guard = link.lock().await;
            if link_guard.status() != LinkStatus::Active {
                return Err(NodeError::LinkClosed);
            }
            link_guard
                .data_packet(data)
                .map_err(|e| NodeError::PacketError(format!("{:?}", e)))?
        };

        let t = self.transport.lock().await;
        t.send_packet(packet).await;
        Ok(())
    }

    /// Receive a message from the network.
    ///
    /// Blocks until a message is received or timeout expires.
    pub async fn recv_message(
        &self,
        timeout_duration: Duration,
    ) -> Result<IncomingMessage, NodeError> {
        let mut rx = self.message_tx.subscribe();
        timeout(timeout_duration, async {
            rx.recv().await.map_err(|_| NodeError::ChannelClosed)
        })
        .await
        .map_err(|_| NodeError::Timeout)?
    }

    /// Spawn the background event processing task.
    async fn spawn_event_task(
        transport: Arc<Mutex<Transport>>,
        links: Arc<Mutex<HashMap<AddressHash, Arc<Mutex<Link>>>>>,
        known_destinations: Arc<Mutex<HashMap<AddressHash, DestinationDesc>>>,
        message_tx: broadcast::Sender<IncomingMessage>,
        announce_tx: broadcast::Sender<AddressHash>,
        link_activation_tx: broadcast::Sender<LinkActivationEvent>,
        cancel: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        // Get channel receivers
        let (mut announces, mut in_link_events, mut out_link_events) = {
            let t = transport.lock().await;
            (
                t.recv_announces().await,
                t.in_link_events(),
                t.out_link_events(),
            )
        };

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        debug!("Node event task shutting down");
                        break;
                    }

                    // Handle incoming announces
                    result = announces.recv() => {
                        match result {
                            Ok(announce) => {
                                let dest = announce.destination.lock().await;
                                let hash = dest.desc.address_hash;
                                let desc = dest.desc;
                                drop(dest);

                                debug!("Node received announce from {}", format_hash_static(&hash));

                                // Store destination
                                {
                                    let mut known = known_destinations.lock().await;
                                    known.insert(hash, desc);
                                }

                                // Notify waiters
                                let _ = announce_tx.send(hash);
                            }
                            Err(e) => {
                                warn!("Announce channel error: {}", e);
                            }
                        }
                    }

                    // Handle incoming link events
                    result = in_link_events.recv() => {
                        if let Ok(event) = result {
                            handle_link_event(
                                event,
                                "inbound",
                                &links,
                                &message_tx,
                                &link_activation_tx,
                            ).await;
                        }
                    }

                    // Handle outgoing link events
                    result = out_link_events.recv() => {
                        if let Ok(event) = result {
                            handle_link_event(
                                event,
                                "outbound",
                                &links,
                                &message_tx,
                                &link_activation_tx,
                            ).await;
                        }
                    }
                }
            }
        })
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Handle a link event (shared logic for inbound and outbound).
async fn handle_link_event(
    event: reticulum::destination::link::LinkEventData,
    direction: &str,
    links: &LinkMap,
    message_tx: &broadcast::Sender<IncomingMessage>,
    link_activation_tx: &broadcast::Sender<LinkActivationEvent>,
) {
    match event.event {
        LinkEvent::Activated => {
            debug!(
                "{} link activated: {}",
                direction,
                format_hash_static(&event.id)
            );
            let _ = link_activation_tx.send(LinkActivationEvent::Activated(event.id));
        }
        LinkEvent::Data(payload) => {
            debug!("{} data from {}", direction, format_hash_static(&event.id));
            let _ = message_tx.send(IncomingMessage {
                from: event.id,
                data: payload.as_slice().to_vec(),
            });
        }
        LinkEvent::Closed => {
            debug!(
                "{} link closed: {}",
                direction,
                format_hash_static(&event.id)
            );
            links.lock().await.remove(&event.id);
            let _ = link_activation_tx.send(LinkActivationEvent::Closed(event.id));
        }
    }
}

/// Format an address hash for logging (first 8 hex chars).
fn format_hash_static(hash: &AddressHash) -> String {
    hash.to_hex_string().chars().take(8).collect()
}

#[cfg(test)]
#[cfg(not(feature = "esp32"))]
mod tests {
    use super::*;
    use log::info;

    // Try both nodes on the same testnet server
    const TESTNET_SERVER_A: &str = "dublin.connect.reticulum.network:4965";
    const TESTNET_SERVER_B: &str = "dublin.connect.reticulum.network:4965";
    const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(60);
    const LINK_TIMEOUT: Duration = Duration::from_secs(60);
    const MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);

    /// Two-node communication test.
    ///
    /// This test validates end-to-end communication:
    /// 1. Both nodes connect to testnet
    /// 2. Both announce their destinations
    /// 3. Node A receives Node B's announce
    /// 4. Node A creates link to Node B
    /// 5. Node A sends message via link
    /// 6. Node B receives the message
    ///
    /// # Known Issue
    ///
    /// Currently failing due to testnet routing: when two nodes connect from
    /// the same IP address, the testnet server appears unable to route directed
    /// packets (like link requests) to the correct client. Broadcast packets
    /// (announces) work fine because they go to all clients.
    ///
    /// This may be a limitation of:
    /// - The reticulum-rs library's interface management
    /// - The testnet server's client disambiguation
    /// - Running multiple Transport instances in the same process
    ///
    /// Run manually with: `cargo test test_two_node -- --ignored --nocapture`
    #[test]
    #[ignore = "testnet routing issue with multiple clients from same IP"]
    fn test_two_node_communication() {
        // Use multi-threaded runtime to ensure both nodes' event loops run concurrently
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Failed to create runtime");

        rt.block_on(async {
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .is_test(true)
                .try_init()
                .ok();

            info!("=== Two-Node Integration Test ===");

            // Create two nodes connected to different testnet servers
            info!("Creating Node A (Dublin)...");
            let node_a = Node::new("node_a", TESTNET_SERVER_A).await;
            info!(
                "Node A hash: {}",
                format_hash_static(&node_a.address_hash())
            );

            info!("Creating Node B (BetweenTheBorders)...");
            let node_b = Node::new("node_b", TESTNET_SERVER_B).await;
            info!(
                "Node B hash: {}",
                format_hash_static(&node_b.address_hash())
            );

            // Both nodes announce
            info!("Both nodes announcing...");
            node_a.announce().await;
            node_b.announce().await;

            // Node A waits for Node B's announce
            info!("Node A waiting for Node B's announce...");
            let dest_b = node_a
                .wait_for_announce(node_b.address_hash(), ANNOUNCE_TIMEOUT)
                .await
                .expect("Failed to receive Node B's announce");
            info!("Node A received Node B's announce!");

            // Node A creates link to Node B
            info!("Node A creating link to Node B...");
            node_a
                .create_link(dest_b, LINK_TIMEOUT)
                .await
                .expect("Failed to create link");
            info!("Link established!");

            // Node A sends message
            let test_message = b"Hello from Node A!";
            info!(
                "Node A sending message: {:?}",
                std::str::from_utf8(test_message)
            );
            node_a
                .send_message(node_b.address_hash(), test_message)
                .await
                .expect("Failed to send message");

            // Node B receives message
            info!("Node B waiting for message...");
            let msg = node_b
                .recv_message(MESSAGE_TIMEOUT)
                .await
                .expect("Failed to receive message");

            info!("Node B received: {:?}", std::str::from_utf8(&msg.data));
            assert_eq!(msg.data, test_message, "Message content should match");

            info!("=== Two-Node Integration Test PASSED ===");
        });
    }
}

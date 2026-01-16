//! Unified Reticulum node binary with chat interface.
//!
//! Runs on both ESP32 and host platforms:
//! - **Host**: `cargo run --bin node`
//! - **ESP32**: `cargo espflash flash --bin node --features esp32 --release`
//!
//! ## Chat Commands
//!
//! Connect via serial monitor and type commands:
//! - `msg <id> <text>` - Send message to destination
//! - `broadcast <text>` - Send to all known destinations
//! - `list` - Show known destinations
//! - `status` - Show node status
//! - `help` - Show help
//!
//! ## Endpoints
//!
//! - Stats: http://localhost:8080/stats
//!
//! ## Lock Ordering
//!
//! To prevent deadlocks, when acquiring multiple locks, acquire in this order:
//! 1. `chat_state` - chat state and destination cache
//! 2. `pending_messages` - queued messages for pending links
//! 3. `links` - active link cache
//! 4. `transport` - network transport
//! 5. Individual `Link` (via `Arc<Mutex<Link>>`)
//!
//! This is a partial order: you don't need all locks for every operation.
//! Most operations only need one or two locks. The rule is: if you need
//! locks A and B, and A comes before B in the list, acquire A first.
//!
//! Examples:
//! - Sending a message: lock `links`, then lock the specific `Link`
//! - Processing announce: lock `chat_state` only
//! - Creating a link: lock `transport` only (link added to cache separately)
//! - Draining pending messages: lock `pending_messages`, then `links`, then `Link`

use log::{debug, error, info, warn};
use reticulum::destination::link::{Link, LinkEvent, LinkStatus};
use reticulum::destination::{DestinationDesc, DestinationName, SingleInputDestination};
use reticulum::hash::AddressHash;
use reticulum::iface::tcp_client::TcpClient;
use reticulum::transport::{Transport, TransportConfig};
use reticulum_rs_esp32::chat::{self, ChatCommand, ChatState};
use reticulum_rs_esp32::message_queue::{QueuedMessage, MAX_QUEUED_MESSAGES_PER_DEST};
use reticulum_rs_esp32::{NodeStats, StatsServer, DEFAULT_STATS_PORT};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Type alias for the shared link cache to avoid clippy complexity warnings.
type LinkCache = Arc<Mutex<HashMap<AddressHash, Arc<Mutex<Link>>>>>;

/// Type alias for pending message queues per destination.
type PendingMessages = Arc<Mutex<HashMap<AddressHash, Vec<QueuedMessage>>>>;

/// Default testnet server. Dublin chosen for geographic diversity from
/// Frankfurt (the other main server). See `src/testnet/config.rs` for alternatives.
const TESTNET_SERVER: &str = "dublin.connect.reticulum.network:4965";

/// How often to re-announce our presence to the network.
/// 5 minutes balances network traffic (announces are broadcast) with
/// keeping the network aware of our presence. Reticulum default is similar.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(300);

/// Maximum concurrent links to prevent memory exhaustion.
/// Each Link holds crypto state (keys, nonces) and buffers. On ESP32 with
/// 512KB SRAM, 20 links is conservative but safe. Increase cautiously based
/// on profiling actual memory usage on device.
const MAX_CONCURRENT_LINKS: usize = 20;

/// How often to check for and remove expired queued messages.
/// 10 seconds is frequent enough to prevent stale message buildup but
/// infrequent enough to avoid unnecessary lock contention.
const QUEUE_CLEANUP_INTERVAL: Duration = Duration::from_secs(10);

// ESP32: Initialize ESP-IDF before anything else
#[cfg(feature = "esp32")]
fn platform_init() {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    info!("ESP-IDF initialized");
}

// Host: Just initialize env_logger
#[cfg(not(feature = "esp32"))]
fn platform_init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}

/// Print a message to stdout (for chat output).
fn print_chat(msg: &str) {
    println!("{}", msg);
    let _ = std::io::stdout().flush();
}

/// Print the prompt.
fn print_prompt() {
    print!("> ");
    let _ = std::io::stdout().flush();
}

/// Print a message followed by the prompt (common pattern for async responses).
fn print_chat_with_prompt(msg: &str) {
    print_chat(msg);
    print_prompt();
}

/// Spawn the network task that handles announces, link events, and message queuing.
///
/// Returns a JoinHandle for the spawned task.
fn spawn_network_task(
    transport: Arc<Mutex<Transport>>,
    stats: Arc<NodeStats>,
    cancel: CancellationToken,
    chat_state: Arc<Mutex<ChatState>>,
    links: LinkCache,
    pending_messages: PendingMessages,
    destination: Arc<Mutex<SingleInputDestination>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Get all channel receivers in a single lock acquisition
        let (mut announces, mut in_link_events, mut out_link_events) = {
            let t = transport.lock().await;
            (
                t.recv_announces().await,
                t.in_link_events(),
                t.out_link_events(),
            )
        };

        let mut announce_timer = tokio::time::interval(ANNOUNCE_INTERVAL);
        announce_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        announce_timer.tick().await; // Skip first

        let mut queue_cleanup_timer = tokio::time::interval(QUEUE_CLEANUP_INTERVAL);
        queue_cleanup_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        queue_cleanup_timer.tick().await; // Skip first

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Network task shutting down");
                    break;
                }

                // Periodic re-announcement
                _ = announce_timer.tick() => {
                    debug!("Sending periodic announce...");
                    let t = transport.lock().await;
                    t.send_announce(&destination, None).await;
                    stats.testnet.record_tx();
                }

                // Periodic cleanup of expired queued messages
                _ = queue_cleanup_timer.tick() => {
                    let mut pending = pending_messages.lock().await;
                    let mut total_expired = 0;

                    // Remove expired messages from each queue
                    pending.retain(|_hash, messages| {
                        let before = messages.len();
                        messages.retain(|m| !m.is_expired());
                        total_expired += before - messages.len();
                        !messages.is_empty() // Remove entry if queue is now empty
                    });

                    if total_expired > 0 {
                        debug!("Expired {} stale queued message(s)", total_expired);
                        stats.queue.expired_messages.fetch_add(total_expired, Ordering::Relaxed);
                        // Use saturating_sub to prevent underflow in case of race conditions
                        stats.queue.queued_messages.fetch_update(
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                            |val| Some(val.saturating_sub(total_expired)),
                        ).ok();
                    }
                }

                // Handle incoming announces
                result = announces.recv() => {
                    match result {
                        Ok(announce) => {
                            let dest = announce.destination.lock().await;
                            let hash = dest.desc.address_hash;
                            let desc = dest.desc;
                            drop(dest); // Release lock

                            debug!("Received announce: {:?}", hash);
                            stats.testnet.record_rx();

                            // Add to chat state (only increment cache size if actually added)
                            let added = {
                                let mut state = chat_state.lock().await;
                                state.add_destination(hash, desc)
                            };
                            if added {
                                stats.routing.announce_cache_size.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(e) => {
                            warn!("Announce channel error: {}", e);
                        }
                    }
                }

                // Handle incoming link data
                result = in_link_events.recv() => {
                    if let Ok(event) = result {
                        match event.event {
                            LinkEvent::Activated => {
                                debug!("Inbound link activated: {:?}", event.id);
                            }
                            LinkEvent::Data(payload) => {
                                // Display incoming message
                                let msg = chat::format_incoming_message(
                                    &event.id,
                                    payload.as_slice()
                                );
                                print_chat_with_prompt(&msg);
                            }
                            LinkEvent::Closed => {
                                debug!("Inbound link closed: {:?}", event.id);
                                // Remove closed link from cache
                                let mut links_guard = links.lock().await;
                                links_guard.remove(&event.id);
                            }
                        }
                    }
                }

                // Handle outgoing link events
                result = out_link_events.recv() => {
                    if let Ok(event) = result {
                        match event.event {
                            LinkEvent::Activated => {
                                debug!("Outbound link activated: {:?}", event.id);
                                // Flush any queued messages for this destination
                                let messages = {
                                    let mut pending = pending_messages.lock().await;
                                    pending.remove(&event.id).unwrap_or_default()
                                };

                                let total_queued = messages.len();
                                if messages.is_empty() {
                                    continue;
                                }

                                // Filter out expired messages
                                let (valid, expired): (Vec<_>, Vec<_>) =
                                    messages.into_iter().partition(|m| !m.is_expired());

                                if !expired.is_empty() {
                                    debug!(
                                        "Dropped {} expired message(s) for {:?}",
                                        expired.len(),
                                        event.id
                                    );
                                    stats.queue.expired_messages.fetch_add(expired.len(), Ordering::Relaxed);
                                }

                                // Update queued_messages count (all removed from queue)
                                // Use saturating_sub to prevent underflow in case of race conditions
                                stats.queue.queued_messages.fetch_update(
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                    |val| Some(val.saturating_sub(total_queued)),
                                ).ok();

                                if valid.is_empty() {
                                    continue;
                                }

                                // Get the link for sending
                                let link = {
                                    let links_guard = links.lock().await;
                                    links_guard.get(&event.id).cloned()
                                };
                                let Some(link) = link else { continue };

                                // Send queued messages, checking link status before each send
                                // to handle the case where link closes during processing
                                let mut sent = 0;
                                for msg in &valid {
                                    let link_guard = link.lock().await;
                                    if link_guard.status() != LinkStatus::Active {
                                        debug!("Link closed while sending queued messages");
                                        break;
                                    }
                                    if let Ok(packet) =
                                        link_guard.data_packet(msg.text().as_bytes())
                                    {
                                        drop(link_guard);
                                        let t = transport.lock().await;
                                        t.send_packet(packet).await;
                                        stats.testnet.record_tx();
                                        sent += 1;
                                    }
                                }

                                if sent > 0 {
                                    print_chat_with_prompt(&format!(
                                        "Link ready, sent {} queued message(s)",
                                        sent
                                    ));
                                }
                            }
                            LinkEvent::Data(payload) => {
                                // Response on outbound link
                                let msg = chat::format_incoming_message(
                                    &event.id,
                                    payload.as_slice()
                                );
                                print_chat_with_prompt(&msg);
                            }
                            LinkEvent::Closed => {
                                debug!("Outbound link closed: {:?}", event.id);
                                // Drop pending messages first (per lock ordering: pending_messages before links)
                                let mut pending = pending_messages.lock().await;
                                let dropped_count = pending.remove(&event.id).map_or(0, |d| d.len());
                                drop(pending);

                                // Then remove closed link from cache
                                let mut links_guard = links.lock().await;
                                links_guard.remove(&event.id);
                                drop(links_guard);

                                if dropped_count > 0 {
                                    stats.queue.dropped_on_close.fetch_add(dropped_count, Ordering::Relaxed);
                                    // Use saturating_sub to prevent underflow in case of race conditions
                                    stats.queue.queued_messages.fetch_update(
                                        Ordering::Relaxed,
                                        Ordering::Relaxed,
                                        |val| Some(val.saturating_sub(dropped_count)),
                                    ).ok();
                                    print_chat_with_prompt(&format!(
                                        "Link closed, {} queued message(s) dropped",
                                        dropped_count
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    platform_init();

    info!("=== Reticulum Node starting ===");

    #[cfg(feature = "esp32")]
    info!("Platform: ESP32");
    #[cfg(not(feature = "esp32"))]
    info!("Platform: Host");

    // Load or create node identity (persisted across restarts)
    #[cfg(feature = "esp32")]
    let identity = {
        let mut nvs =
            reticulum_rs_esp32::persistence::init_nvs().expect("Failed to initialize NVS");
        reticulum_rs_esp32::persistence::load_or_create_identity(&mut nvs)
            .expect("Failed to load/create identity")
    };

    #[cfg(not(feature = "esp32"))]
    let identity = reticulum_rs_esp32::persistence_host::load_or_create_identity()
        .expect("Failed to load/create identity");

    let identity_hash = identity.address_hash().to_string();
    let identity_short = identity_hash.chars().take(8).collect::<String>();
    info!("Node identity: {}", identity_hash);

    // Start stats server
    let stats = Arc::new(NodeStats::new(identity_hash.clone()));
    let _stats_server = match StatsServer::start(None, DEFAULT_STATS_PORT, stats.clone()) {
        Ok(server) => {
            info!(
                "Stats server at http://localhost:{}/stats",
                DEFAULT_STATS_PORT
            );
            Some(server)
        }
        Err(e) => {
            warn!("Failed to start stats server: {}", e);
            None
        }
    };

    // Initialize chat state
    let chat_state = Arc::new(Mutex::new(ChatState::new(identity_short.clone())));

    // Create reticulum transport
    let transport = Arc::new(Mutex::new(Transport::new(TransportConfig::default())));

    // Connect to testnet (may fail if no WiFi configured - that's OK for local testing)
    info!("Connecting to testnet: {}", TESTNET_SERVER);
    {
        let t = transport.lock().await;
        t.iface_manager()
            .lock()
            .await
            .spawn(TcpClient::new(TESTNET_SERVER), TcpClient::spawn);
    }
    info!("Testnet interface spawned");

    // Create and register our destination
    let dest_name = DestinationName::new("reticulum_rs_esp32", "chat");
    let destination = {
        let mut t = transport.lock().await;
        t.add_destination(identity, dest_name).await
    };

    // Announce our presence
    info!("Announcing to network...");
    {
        let t = transport.lock().await;
        t.send_announce(&destination, None).await;
    }
    stats.testnet.record_tx();
    info!("Announce sent");

    // LoRa interface placeholder
    #[cfg(feature = "esp32")]
    {
        info!("LoRa interface: not initialized (requires hardware testing)");
    }

    // Set up cancellation
    let cancel = CancellationToken::new();

    // Track active links for messaging
    let links: LinkCache = Arc::new(Mutex::new(HashMap::new()));

    // Queue for messages sent to pending links (sent when link activates)
    let pending_messages: PendingMessages = Arc::new(Mutex::new(HashMap::new()));

    // Spawn network task (announces, incoming messages, link events)
    let network_task = spawn_network_task(
        transport.clone(),
        stats.clone(),
        cancel.clone(),
        chat_state.clone(),
        links.clone(),
        pending_messages.clone(),
        destination.clone(),
    );

    // Print welcome message
    print_chat("");
    print_chat("=== Reticulum Chat ===");
    print_chat(&format!("Identity: {}", identity_short));
    print_chat("Type 'help' for commands");
    print_chat("");
    print_prompt();

    // Spawn stdin reader task
    let stdin_transport = transport.clone();
    let stdin_chat = chat_state.clone();
    let stdin_stats = stats.clone();
    let stdin_links = links.clone();
    let stdin_pending = pending_messages.clone();
    let stdin_cancel = cancel.clone();

    let stdin_task = tokio::task::spawn_blocking(move || {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();

        while !stdin_cancel.is_cancelled() {
            if let Some(Ok(line)) = lines.next() {
                let cmd = ChatCommand::parse(&line);

                // We need to handle commands in a blocking context
                // Use a channel to send commands to an async handler
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    handle_command(
                        cmd,
                        &stdin_transport,
                        &stdin_chat,
                        &stdin_stats,
                        &stdin_links,
                        &stdin_pending,
                    )
                    .await;
                });

                print_prompt();
            }
        }
    });

    // Wait for shutdown
    #[cfg(not(feature = "esp32"))]
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            print_chat("\nShutting down...");
            cancel.cancel();
        }
        result = network_task => {
            if let Err(e) = result {
                error!("Network task error: {}", e);
            }
        }
        result = stdin_task => {
            if let Err(e) = result {
                error!("Stdin task error: {}", e);
            }
        }
    }

    #[cfg(feature = "esp32")]
    {
        let _ = cancel;
        tokio::select! {
            result = network_task => {
                if let Err(e) = result {
                    error!("Network task error: {}", e);
                }
            }
            result = stdin_task => {
                if let Err(e) = result {
                    error!("Stdin task error: {}", e);
                }
            }
        }
    }

    info!("Shutdown complete");
}

/// Result of attempting to get or create a link.
enum GetLinkResult {
    /// Found existing link.
    Existing(Arc<Mutex<Link>>),
    /// Created new link.
    Created(Arc<Mutex<Link>>),
    /// Concurrent link limit reached.
    LimitReached,
}

/// Get an existing link or create a new one.
///
/// Returns the link if found or created, or LimitReached if at capacity.
async fn get_or_create_link(
    links: &LinkCache,
    transport: &Arc<Mutex<Transport>>,
    hash: AddressHash,
    descriptor: DestinationDesc,
) -> GetLinkResult {
    let mut links_guard = links.lock().await;

    if let Some(link) = links_guard.get(&hash) {
        return GetLinkResult::Existing(link.clone());
    }

    if links_guard.len() >= MAX_CONCURRENT_LINKS {
        return GetLinkResult::LimitReached;
    }

    // Create new link
    let t = transport.lock().await;
    let new_link = t.link(descriptor).await;
    links_guard.insert(hash, new_link.clone());
    GetLinkResult::Created(new_link)
}

/// Handle a parsed chat command.
async fn handle_command(
    cmd: ChatCommand,
    transport: &Arc<Mutex<Transport>>,
    chat_state: &Arc<Mutex<ChatState>>,
    stats: &Arc<NodeStats>,
    links: &LinkCache,
    pending_messages: &PendingMessages,
) {
    match cmd {
        ChatCommand::Message { dest_id, text } => {
            let state = chat_state.lock().await;
            if let Some(dest) = state.get_destination(&dest_id) {
                let hash = dest.hash;
                let descriptor = dest.descriptor;
                let display_name = dest.display_name.clone();
                drop(state);

                // Get or create link
                let link = match get_or_create_link(links, transport, hash, descriptor).await {
                    GetLinkResult::Existing(link) => link,
                    GetLinkResult::Created(link) => {
                        print_chat(&format!("Creating link to {}...", display_name));
                        link
                    }
                    GetLinkResult::LimitReached => {
                        print_chat("Too many active links. Wait for some to close.");
                        return;
                    }
                };

                // Check link state and queue atomically to prevent TOCTOU race.
                // If we check status, release lock, then queue, the link could activate
                // in between and our queued message would never be sent.
                let mut pending = pending_messages.lock().await;
                let link_guard = link.lock().await;
                let status = link_guard.status();

                if status != LinkStatus::Active {
                    drop(link_guard);
                    // Queue message for when link activates
                    if status == LinkStatus::Stale || status == LinkStatus::Closed {
                        drop(pending);
                        let status_str = if status == LinkStatus::Stale {
                            "stale"
                        } else {
                            "closed"
                        };
                        print_chat(&format!(
                            "Link to {} is {}, cannot queue message",
                            display_name, status_str
                        ));
                        return;
                    }

                    // Queue for Pending or Handshake states
                    let queue = pending.entry(hash).or_default();
                    if queue.len() >= MAX_QUEUED_MESSAGES_PER_DEST {
                        print_chat(&format!(
                            "Queue full for {} ({} messages), try again shortly",
                            display_name, MAX_QUEUED_MESSAGES_PER_DEST
                        ));
                        return;
                    }
                    queue.push(QueuedMessage::new(text));
                    stats.queue.queued_messages.fetch_add(1, Ordering::Relaxed);
                    let queue_len = queue.len();
                    drop(pending);
                    print_chat(&format!(
                        "Link establishing, message queued ({} pending)",
                        queue_len
                    ));
                    return;
                }
                drop(pending);

                // Send message via active link (link_guard still held)
                match link_guard.data_packet(text.as_bytes()) {
                    Ok(packet) => {
                        drop(link_guard);
                        let t = transport.lock().await;
                        t.send_packet(packet).await;
                        stats.testnet.record_tx();
                        print_chat(&format!("Sent to {}", display_name));
                    }
                    Err(e) => {
                        print_chat(&format!("Error creating packet: {:?}", e));
                    }
                }
            } else {
                print_chat(&format!("Unknown destination: {}", dest_id));
                print_chat("Use 'list' to see known destinations");
            }
        }

        ChatCommand::Broadcast { text } => {
            let state = chat_state.lock().await;
            let destinations: Vec<_> = state.all_destinations().to_vec();
            drop(state);

            if destinations.is_empty() {
                print_chat("No known destinations. Wait for announces...");
                return;
            }

            // Phase 1: Collect all packets (brief lock per link)
            let mut packets = Vec::new();
            let mut skipped = 0;
            for dest in destinations {
                // Get or create link
                let link =
                    match get_or_create_link(links, transport, dest.hash, dest.descriptor).await {
                        GetLinkResult::Existing(link) | GetLinkResult::Created(link) => link,
                        GetLinkResult::LimitReached => {
                            skipped += 1;
                            continue;
                        }
                    };

                // Only send on active links
                let link_guard = link.lock().await;
                if link_guard.status() != LinkStatus::Active {
                    skipped += 1;
                    continue;
                }

                if let Ok(packet) = link_guard.data_packet(text.as_bytes()) {
                    packets.push(packet);
                }
                // link_guard dropped here
            }

            // Phase 2: Send all packets in single transport lock
            let sent = packets.len();
            if !packets.is_empty() {
                let t = transport.lock().await;
                for packet in packets {
                    t.send_packet(packet).await;
                    stats.testnet.record_tx();
                }
            }

            if skipped > 0 {
                print_chat(&format!(
                    "Broadcast sent to {} destination(s), {} skipped (not ready or limit)",
                    sent, skipped
                ));
            } else {
                print_chat(&format!("Broadcast sent to {} destination(s)", sent));
            }
        }

        ChatCommand::List => {
            let state = chat_state.lock().await;
            print_chat(&state.format_list());
        }

        ChatCommand::Status => {
            let state = chat_state.lock().await;
            print_chat(&state.format_status());
        }

        ChatCommand::Help => {
            print_chat(chat::HELP_TEXT);
        }

        ChatCommand::Unknown(msg) => {
            if !msg.is_empty() {
                print_chat(&msg);
            }
        }
    }
}

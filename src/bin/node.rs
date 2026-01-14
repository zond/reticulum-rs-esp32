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
//! To prevent deadlocks, locks must be acquired in this order:
//! 1. `chat_state` - chat state and destination cache
//! 2. `pending_messages` - queued messages for pending links
//! 3. `links` - active link cache
//! 4. `transport` - network transport
//! 5. Individual `Link` (via `Arc<Mutex<Link>>`)
//!
//! Always release locks in reverse order when possible.

use log::{debug, error, info, warn};
use reticulum::destination::link::{Link, LinkEvent, LinkStatus};
use reticulum::destination::{DestinationDesc, DestinationName};
use reticulum::hash::AddressHash;
use reticulum::iface::tcp_client::TcpClient;
use reticulum::transport::{Transport, TransportConfig};
use reticulum_rs_esp32::chat::{self, ChatCommand, ChatState};
use reticulum_rs_esp32::{NodeStats, StatsServer, DEFAULT_STATS_PORT};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const TESTNET_SERVER: &str = "dublin.connect.reticulum.network:4965";

/// How often to re-announce our presence to the network.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// Maximum concurrent links to prevent memory exhaustion.
/// Each Link holds crypto state (keys, nonces) and buffers. On ESP32 with
/// 512KB SRAM, 20 links is conservative but safe. Increase cautiously based
/// on profiling actual memory usage on device.
const MAX_CONCURRENT_LINKS: usize = 20;

/// Maximum queued messages per destination to prevent memory exhaustion.
/// Messages are queued when sent to a link that's still establishing.
/// Once the link activates, queued messages are sent automatically.
const MAX_QUEUED_MESSAGES_PER_DEST: usize = 5;

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
    let links: Arc<Mutex<HashMap<AddressHash, Arc<Mutex<Link>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Queue for messages sent to pending links (sent when link activates)
    let pending_messages: Arc<Mutex<HashMap<AddressHash, Vec<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Spawn network task (announces, incoming messages, link events)
    let network_transport = transport.clone();
    let network_stats = stats.clone();
    let network_cancel = cancel.clone();
    let network_chat = chat_state.clone();
    let network_links = links.clone();
    let network_pending = pending_messages.clone();
    let network_destination = destination.clone();

    let network_task = tokio::spawn(async move {
        let mut announces = {
            let t = network_transport.lock().await;
            t.recv_announces().await
        };

        let mut in_link_events = {
            let t = network_transport.lock().await;
            t.in_link_events()
        };

        let mut out_link_events = {
            let t = network_transport.lock().await;
            t.out_link_events()
        };

        let mut announce_timer = tokio::time::interval(ANNOUNCE_INTERVAL);
        announce_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        announce_timer.tick().await; // Skip first

        loop {
            tokio::select! {
                _ = network_cancel.cancelled() => {
                    info!("Network task shutting down");
                    break;
                }

                // Periodic re-announcement
                _ = announce_timer.tick() => {
                    debug!("Sending periodic announce...");
                    let t = network_transport.lock().await;
                    t.send_announce(&network_destination, None).await;
                    network_stats.testnet.record_tx();
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
                            network_stats.testnet.record_rx();

                            // Add to chat state (only increment cache size if actually added)
                            let added = {
                                let mut state = network_chat.lock().await;
                                state.add_destination(hash, desc)
                            };
                            if added {
                                network_stats.routing.announce_cache_size.fetch_add(1, Ordering::Relaxed);
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
                                print_chat(&msg);
                                print_prompt();
                            }
                            LinkEvent::Closed => {
                                debug!("Inbound link closed: {:?}", event.id);
                                // Remove closed link from cache
                                let mut links_guard = network_links.lock().await;
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
                                    let mut pending = network_pending.lock().await;
                                    pending.remove(&event.id).unwrap_or_default()
                                };
                                if !messages.is_empty() {
                                    // Get the link and send queued messages
                                    let link = {
                                        let links_guard = network_links.lock().await;
                                        links_guard.get(&event.id).cloned()
                                    };
                                    if let Some(link) = link {
                                        let mut sent = 0;
                                        for msg in &messages {
                                            let link_guard = link.lock().await;
                                            if let Ok(packet) = link_guard.data_packet(msg.as_bytes()) {
                                                drop(link_guard);
                                                let t = network_transport.lock().await;
                                                t.send_packet(packet).await;
                                                network_stats.testnet.record_tx();
                                                sent += 1;
                                            }
                                        }
                                        print_chat(&format!(
                                            "Link ready, sent {} queued message(s)",
                                            sent
                                        ));
                                        print_prompt();
                                    }
                                }
                            }
                            LinkEvent::Data(payload) => {
                                // Response on outbound link
                                let msg = chat::format_incoming_message(
                                    &event.id,
                                    payload.as_slice()
                                );
                                print_chat(&msg);
                                print_prompt();
                            }
                            LinkEvent::Closed => {
                                debug!("Outbound link closed: {:?}", event.id);
                                // Remove closed link from cache
                                let mut links_guard = network_links.lock().await;
                                links_guard.remove(&event.id);
                                // Drop any pending messages (link failed before activating)
                                let mut pending = network_pending.lock().await;
                                if let Some(dropped) = pending.remove(&event.id) {
                                    if !dropped.is_empty() {
                                        print_chat(&format!(
                                            "Link closed, {} queued message(s) dropped",
                                            dropped.len()
                                        ));
                                        print_prompt();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

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
    links: &Arc<Mutex<HashMap<AddressHash, Arc<Mutex<Link>>>>>,
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
    links: &Arc<Mutex<HashMap<AddressHash, Arc<Mutex<Link>>>>>,
    pending_messages: &Arc<Mutex<HashMap<AddressHash, Vec<String>>>>,
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
                    queue.push(text);
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

            let mut sent = 0;
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
                    drop(link_guard);
                    let t = transport.lock().await;
                    t.send_packet(packet).await;
                    stats.testnet.record_tx();
                    sent += 1;
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

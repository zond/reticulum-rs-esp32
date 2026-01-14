//! Unified Reticulum node binary.
//!
//! Runs on both ESP32 and host platforms:
//! - **Host**: `cargo run --bin node`
//! - **ESP32**: `cargo espflash flash --bin node --features esp32 --release`
//!
//! Stats endpoint: http://localhost:8080/stats

use log::{error, info, warn};
use reticulum::destination::DestinationName;
use reticulum::iface::tcp_client::TcpClient;
use reticulum::transport::{Transport, TransportConfig};
use reticulum_rs_esp32::{NodeStats, StatsServer, DEFAULT_STATS_PORT};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const TESTNET_SERVER: &str = "dublin.connect.reticulum.network:4965";

/// How often to re-announce our presence to the network.
/// Reticulum announces typically have a TTL, so periodic re-announcement
/// ensures our paths stay fresh in the network.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

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
    info!("Node identity: {}", identity_hash);

    // Start stats server (held until end of main for proper cleanup)
    let stats = Arc::new(NodeStats::new(identity_hash));
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

    // Create reticulum transport
    let mut transport = Transport::new(TransportConfig::default());

    // Connect to testnet
    info!("Connecting to testnet: {}", TESTNET_SERVER);
    transport
        .iface_manager()
        .lock()
        .await
        .spawn(TcpClient::new(TESTNET_SERVER), TcpClient::spawn);

    info!("Connected to testnet");

    // Create and register our destination
    let dest_name = DestinationName::new("reticulum_rs_esp32", "node");
    let destination = transport.add_destination(identity, dest_name).await;

    // Announce our presence to the network
    info!("Announcing to network...");
    transport.send_announce(&destination, None).await;
    stats.testnet.record_tx();
    info!("Announce sent, listening for other announces...");

    // TODO: Initialize LoRa interface on ESP32
    #[cfg(feature = "esp32")]
    {
        info!("TODO: LoRa interface not yet integrated with reticulum transport");
        // Future: spawn LoRa interface here
    }

    // Set up cancellation for graceful shutdown
    let cancel = CancellationToken::new();

    // Spawn main network task (listens for announces + periodic re-announcement)
    let stats_clone = stats.clone();
    let cancel_clone = cancel.clone();
    let network_task = tokio::spawn(async move {
        let mut announces = transport.recv_announces().await;
        let mut announce_timer = tokio::time::interval(ANNOUNCE_INTERVAL);
        // Use Delay behavior to prevent burst re-announcements if ticks are missed
        announce_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Skip the first tick (we already announced at startup)
        announce_timer.tick().await;

        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    info!("Network task shutting down");
                    break;
                }
                // Periodic re-announcement
                _ = announce_timer.tick() => {
                    info!("Sending periodic announce...");
                    transport.send_announce(&destination, None).await;
                    stats_clone.testnet.record_tx();
                }
                // Listen for announces from other nodes
                result = announces.recv() => {
                    match result {
                        Ok(announce) => {
                            let dest = announce.destination.lock().await;
                            let hash = &dest.desc.address_hash;
                            info!("Received announce: {:?}", hash);

                            // Update stats (note: announce_cache_size tracks total received,
                            // not actual cache size - real cache is managed by reticulum-rs)
                            stats_clone.testnet.record_rx();
                            stats_clone.routing.announce_cache_size.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            warn!("Announce channel error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    });

    info!("Node running (Ctrl+C to exit)...");

    // Wait for shutdown signal or task completion
    #[cfg(not(feature = "esp32"))]
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
            cancel.cancel();
        }
        result = network_task => {
            if let Err(e) = result {
                error!("Network task error: {}", e);
            }
        }
    }

    // ESP32: No signal handling (no POSIX signals on espidf), just wait for task.
    // The cancel token is unused but kept for API consistency - the node runs
    // until hardware reset or power loss.
    #[cfg(feature = "esp32")]
    {
        let _ = cancel;
        if let Err(e) = network_task.await {
            error!("Network task error: {}", e);
        }
    }

    info!("Shutdown complete");
}

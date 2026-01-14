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
use tokio_util::sync::CancellationToken;

const TESTNET_SERVER: &str = "dublin.connect.reticulum.network:4965";

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
    info!("Announce sent, listening for other announces...");

    // TODO: Initialize LoRa interface on ESP32
    #[cfg(feature = "esp32")]
    {
        info!("TODO: LoRa interface not yet integrated with reticulum transport");
        // Future: spawn LoRa interface here
    }

    // Set up cancellation for graceful shutdown
    let cancel = CancellationToken::new();

    // Spawn announce listener task
    let stats_clone = stats.clone();
    let cancel_clone = cancel.clone();
    let announce_task = tokio::spawn(async move {
        let mut announces = transport.recv_announces().await;

        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    info!("Announce listener shutting down");
                    break;
                }
                result = announces.recv() => {
                    match result {
                        Ok(announce) => {
                            let destination = announce.destination.lock().await;
                            let hash = &destination.desc.address_hash;
                            info!("Received announce: {:?}", hash);

                            // Update stats
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
        result = announce_task => {
            if let Err(e) = result {
                error!("Announce task error: {}", e);
            }
        }
    }

    // ESP32: No signal handling, just wait for task
    #[cfg(feature = "esp32")]
    {
        let _ = cancel;
        if let Err(e) = announce_task.await {
            error!("Announce task error: {}", e);
        }
    }

    info!("Shutdown complete");
}

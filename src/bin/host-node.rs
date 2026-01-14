//! Host-based Reticulum node for development and testing.
//!
//! This binary runs on the host machine (not ESP32) and provides:
//! - Connection to the Reticulum testnet via TCP
//! - HTTP stats endpoint at http://localhost:8080/stats
//!
//! # Usage
//!
//! ```bash
//! cargo run --bin host-node --features network-host
//! ```

use log::{error, info, warn};
use reticulum_rs_esp32::{
    HostNetwork, NetworkProvider, NodeStats, StatsServer, TestnetTransport, DEFAULT_SERVER,
    DEFAULT_STATS_PORT,
};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== Reticulum Host Node starting ===");

    // Initialize network (always succeeds on host, but kept for API consistency)
    let mut _network = HostNetwork::new();
    if let Err(e) = _network.connect() {
        error!("Network initialization failed: {}", e);
        std::process::exit(1);
    }

    // Create node stats
    let stats = Arc::new(NodeStats::new("host-node".to_string()));

    // Start stats server
    // Keep server alive - variable intentionally unused except for Drop
    // Bind to 0.0.0.0 so localhost works (passing None instead of specific IP)
    let _stats_server = match StatsServer::start(None, DEFAULT_STATS_PORT, stats.clone()) {
        Ok(server) => {
            info!(
                "Stats server running at http://localhost:{}/stats",
                DEFAULT_STATS_PORT
            );
            Some(server)
        }
        Err(e) => {
            warn!("Failed to start stats server: {}", e);
            warn!("Continuing without stats server");
            None
        }
    };

    // Connect to testnet
    info!("Connecting to testnet...");
    let mut transport = match TestnetTransport::connect(DEFAULT_SERVER) {
        Ok(t) => {
            info!("Connected to testnet: {}", t.server_name());
            Some(t)
        }
        Err(e) => {
            error!("Failed to connect to testnet: {}", e);
            error!("The stats server is still running - you can view stats at the URL above");
            None
        }
    };

    info!("Entering main loop (Ctrl+C to exit)...");

    // Main loop
    // TODO: Add graceful shutdown via signal handling (ctrlc crate)
    // TODO: Add exponential backoff for reconnection attempts
    let mut heartbeat_counter = 0u64;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        heartbeat_counter += 1;

        // Check testnet connection
        if let Some(ref t) = transport {
            if t.may_be_connected() {
                info!("Heartbeat #{} - testnet connected", heartbeat_counter);
            } else {
                warn!(
                    "Heartbeat #{} - testnet disconnected, reconnecting...",
                    heartbeat_counter
                );
                transport = TestnetTransport::connect(DEFAULT_SERVER).ok();
            }
        } else {
            // Try to reconnect periodically
            if heartbeat_counter % 6 == 0 {
                info!("Attempting to reconnect to testnet...");
                transport = TestnetTransport::connect(DEFAULT_SERVER).ok();
                if transport.is_some() {
                    info!("Reconnected to testnet");
                }
            }
        }
    }
}

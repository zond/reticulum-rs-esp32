//! HTTP stats server for node monitoring.
//!
//! Provides a simple `/stats` endpoint that returns node statistics as JSON.
//! Uses `tiny_http` which works on both host and ESP32 (via std::net).
//!
//! # Example Response
//!
//! ```json
//! {
//!   "uptime_secs": 3600,
//!   "identity_hash": "/a1b2c3d4.../",
//!   "interfaces": {
//!     "lora": { "tx": 150, "rx": 230 },
//!     "ble": { "tx": 50, "rx": 45 },
//!     "testnet": { "tx": 500, "rx": 480 }
//!   },
//!   "routing": {
//!     "announce_cache_size": 25,
//!     "path_table_size": 8
//!   },
//!   "queue": {
//!     "queued_messages": 3,
//!     "expired_messages": 12,
//!     "dropped_on_close": 5
//!   }
//! }
//! ```

use log::{error, info, warn};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tiny_http::{Method, Response, Server};

/// Default port for the stats server.
pub const DEFAULT_STATS_PORT: u16 = 8080;

/// Statistics for a single interface (packet counts only).
#[derive(Debug, Default)]
pub struct InterfaceStats {
    /// Packets transmitted.
    pub tx: AtomicUsize,
    /// Packets received.
    pub rx: AtomicUsize,
}

impl InterfaceStats {
    /// Create new interface stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a transmitted packet.
    pub fn record_tx(&self) {
        self.tx.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a received packet.
    pub fn record_rx(&self) {
        self.rx.fetch_add(1, Ordering::Relaxed);
    }

    /// Serialize to JSON.
    fn to_json(&self) -> String {
        format!(
            r#"{{"tx":{},"rx":{}}}"#,
            self.tx.load(Ordering::Relaxed),
            self.rx.load(Ordering::Relaxed)
        )
    }
}

/// Routing statistics.
#[derive(Debug, Default)]
pub struct RoutingStats {
    /// Number of entries in the announce cache.
    pub announce_cache_size: AtomicUsize,
    /// Number of entries in the path table.
    pub path_table_size: AtomicUsize,
    /// Number of known destinations.
    pub known_destinations: AtomicUsize,
}

impl RoutingStats {
    /// Create new routing stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize to JSON.
    fn to_json(&self) -> String {
        format!(
            r#"{{"announce_cache_size":{},"path_table_size":{},"known_destinations":{}}}"#,
            self.announce_cache_size.load(Ordering::Relaxed),
            self.path_table_size.load(Ordering::Relaxed),
            self.known_destinations.load(Ordering::Relaxed)
        )
    }
}

/// Message queue statistics for ESP32 memory monitoring.
///
/// Tracks the pending message queue state to help identify memory pressure
/// from slow link establishment or excessive queueing.
#[derive(Debug, Default)]
pub struct QueueStats {
    /// Current total queued messages across all destinations.
    /// This should stay below MAX_QUEUED_MESSAGES_PER_DEST * active_links.
    pub queued_messages: AtomicUsize,
    /// Cumulative count of messages expired due to TTL.
    /// High values may indicate links failing to establish.
    pub expired_messages: AtomicUsize,
    /// Cumulative count of messages dropped when links close.
    /// Normal during link churn, but high sustained values may indicate issues.
    pub dropped_on_close: AtomicUsize,
}

impl QueueStats {
    /// Create new queue stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize to JSON.
    fn to_json(&self) -> String {
        format!(
            r#"{{"queued_messages":{},"expired_messages":{},"dropped_on_close":{}}}"#,
            self.queued_messages.load(Ordering::Relaxed),
            self.expired_messages.load(Ordering::Relaxed),
            self.dropped_on_close.load(Ordering::Relaxed)
        )
    }
}

/// Node statistics container.
///
/// This struct is shared across the application and updated by various components.
/// All fields use atomic types for thread-safe access without locking.
#[derive(Debug)]
pub struct NodeStats {
    /// When the node started.
    start_time: Instant,
    /// Node identity hash (hex string).
    pub identity_hash: String,
    /// LoRa interface statistics.
    pub lora: InterfaceStats,
    /// BLE interface statistics.
    pub ble: InterfaceStats,
    /// Testnet (TCP) interface statistics.
    pub testnet: InterfaceStats,
    /// Routing statistics.
    pub routing: RoutingStats,
    /// Message queue statistics for memory monitoring.
    pub queue: QueueStats,
}

impl NodeStats {
    /// Create new node statistics.
    ///
    /// # Arguments
    ///
    /// * `identity_hash` - The node's identity hash as a hex string
    pub fn new(identity_hash: String) -> Self {
        Self {
            start_time: Instant::now(),
            identity_hash,
            lora: InterfaceStats::new(),
            ble: InterfaceStats::new(),
            testnet: InterfaceStats::new(),
            routing: RoutingStats::new(),
            queue: QueueStats::new(),
        }
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Serialize all statistics to JSON.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"uptime_secs":{},"identity_hash":"{}","interfaces":{{"lora":{},"ble":{},"testnet":{}}},"routing":{},"queue":{}}}"#,
            self.uptime_secs(),
            self.identity_hash,
            self.lora.to_json(),
            self.ble.to_json(),
            self.testnet.to_json(),
            self.routing.to_json(),
            self.queue.to_json()
        )
    }
}

impl Default for NodeStats {
    fn default() -> Self {
        Self::new("unknown".to_string())
    }
}

/// HTTP stats server.
///
/// Runs in a background thread and serves node statistics as JSON.
pub struct StatsServer {
    /// Server thread handle.
    handle: Option<thread::JoinHandle<()>>,
    /// Flag to signal shutdown.
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl StatsServer {
    /// Start the stats server.
    ///
    /// # Arguments
    ///
    /// * `bind_addr` - IP address to bind to (use `None` for 0.0.0.0)
    /// * `port` - Port to listen on
    /// * `stats` - Shared statistics to serve
    ///
    /// # Returns
    ///
    /// A handle to the running server. Drop it to stop the server.
    pub fn start(
        bind_addr: Option<IpAddr>,
        port: u16,
        stats: Arc<NodeStats>,
    ) -> Result<Self, std::io::Error> {
        let addr = match bind_addr {
            Some(ip) => format!("{}:{}", ip, port),
            None => format!("0.0.0.0:{}", port),
        };

        let server = Server::http(&addr)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::AddrInUse, format!("{}", e)))?;

        info!("Stats server listening on http://{}/stats", addr);

        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let handle = thread::spawn(move || {
            Self::run_server(server, stats, shutdown_clone);
        });

        Ok(Self {
            handle: Some(handle),
            shutdown,
        })
    }

    /// Run the server loop.
    ///
    /// # Security Notes
    ///
    /// TODO: For production ESP32 deployment, consider:
    /// - Adding connection limits (ESP32 has limited thread stack space)
    /// - Adding rate limiting to prevent DoS
    /// - Using a single-threaded server or UDP-based stats protocol
    fn run_server(
        server: Server,
        stats: Arc<NodeStats>,
        shutdown: Arc<std::sync::atomic::AtomicBool>,
    ) {
        // Pre-create headers to avoid repeated allocations
        let content_type =
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .expect("static header");
        let location =
            tiny_http::Header::from_bytes(&b"Location"[..], &b"/stats"[..]).expect("static header");
        let allow_get =
            tiny_http::Header::from_bytes(&b"Allow"[..], &b"GET"[..]).expect("static header");

        loop {
            // Use Acquire ordering to ensure we see the shutdown flag from stop()
            if shutdown.load(Ordering::Acquire) {
                info!("Stats server shutting down");
                break;
            }

            match server.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(Some(request)) => {
                    // Only allow GET requests
                    if request.method() != &Method::Get {
                        let response = Response::from_string("Method Not Allowed")
                            .with_status_code(405)
                            .with_header(allow_get.clone());
                        let _ = request.respond(response);
                        continue;
                    }

                    let path = request.url();

                    if path == "/stats" || path == "/stats/" {
                        let json = stats.to_json();
                        let response = Response::from_string(json)
                            .with_header(content_type.clone())
                            .with_status_code(200);

                        if let Err(e) = request.respond(response) {
                            warn!("Failed to send response: {}", e);
                        }
                    } else if path == "/" {
                        // Redirect root to /stats
                        let response = Response::from_string("See /stats for node statistics")
                            .with_status_code(302)
                            .with_header(location.clone());

                        if let Err(e) = request.respond(response) {
                            warn!("Failed to send redirect: {}", e);
                        }
                    } else {
                        // 404 for other paths
                        let response = Response::from_string("Not Found").with_status_code(404);

                        if let Err(e) = request.respond(response) {
                            warn!("Failed to send 404: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    // Timeout, check shutdown flag and continue
                }
                Err(e) => {
                    error!("Server error: {}", e);
                    break;
                }
            }
        }
    }

    /// Stop the server.
    ///
    /// Note: May take up to 100ms due to polling interval.
    pub fn stop(&mut self) {
        // Use Release ordering to ensure the server thread sees this write
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for StatsServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reticulum_rs_esp32_macros::esp32_test;

    #[esp32_test]
    fn test_interface_stats_new() {
        let stats = InterfaceStats::new();
        assert_eq!(stats.tx.load(Ordering::Relaxed), 0);
        assert_eq!(stats.rx.load(Ordering::Relaxed), 0);
    }

    #[esp32_test]
    fn test_interface_stats_record() {
        let stats = InterfaceStats::new();
        stats.record_tx();
        stats.record_rx();
        stats.record_rx();

        assert_eq!(stats.tx.load(Ordering::Relaxed), 1);
        assert_eq!(stats.rx.load(Ordering::Relaxed), 2);
    }

    #[esp32_test]
    fn test_node_stats_json() {
        let stats = NodeStats::new("abc123".to_string());
        let json = stats.to_json();

        assert!(json.contains("\"identity_hash\":\"abc123\""));
        assert!(json.contains("\"uptime_secs\":"));
        assert!(json.contains("\"interfaces\":"));
        assert!(json.contains("\"routing\":"));
        assert!(json.contains("\"queue\":"));
    }

    #[esp32_test]
    fn test_queue_stats_new() {
        let stats = QueueStats::new();
        assert_eq!(stats.queued_messages.load(Ordering::Relaxed), 0);
        assert_eq!(stats.expired_messages.load(Ordering::Relaxed), 0);
        assert_eq!(stats.dropped_on_close.load(Ordering::Relaxed), 0);
    }

    #[esp32_test]
    fn test_queue_stats_json() {
        let stats = QueueStats::new();
        stats.queued_messages.store(5, Ordering::Relaxed);
        stats.expired_messages.store(10, Ordering::Relaxed);
        stats.dropped_on_close.store(3, Ordering::Relaxed);

        let json = stats.to_json();
        assert!(json.contains("\"queued_messages\":5"));
        assert!(json.contains("\"expired_messages\":10"));
        assert!(json.contains("\"dropped_on_close\":3"));
    }

    #[esp32_test]
    fn test_node_stats_uptime() {
        let stats = NodeStats::new("test".to_string());
        std::thread::sleep(std::time::Duration::from_millis(10));
        // Uptime should be at least 0 (might be 0 if very fast)
        assert!(stats.uptime_secs() < 10);
    }
}

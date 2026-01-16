//! Reticulum-rs ESP32 firmware library.
//!
//! This library contains platform-independent components that can be tested
//! on the host machine without ESP32 hardware.

pub mod announce;
pub mod ble;
pub mod chat;
pub mod config;
#[cfg(not(feature = "esp32"))]
pub mod host_utils;
pub mod lora;
pub mod message_queue;
pub mod network;
#[cfg(feature = "esp32")]
pub mod persistence;
#[cfg(not(feature = "esp32"))]
pub mod persistence_host;
pub mod routing;
pub mod testnet;
pub mod wifi;

// Re-export commonly used items
pub use announce::{AnnounceCache, AnnounceCacheConfig, AnnounceEntry};
pub use ble::{Fragment, FragmentError, Fragmenter, Reassembler};
pub use chat::{ChatCommand, ChatState, KnownDestination, HELP_TEXT};
pub use config::{ConfigCommand, ConfigError, WifiConfig, WifiStatus};
pub use lora::{calculate_airtime_ms, calculate_airtime_us, DutyCycleLimiter, LoRaParams};
pub use message_queue::{QueuedMessage, MAX_QUEUED_MESSAGES_PER_DEST, QUEUE_MESSAGE_TTL};
pub use network::{NetworkError, NetworkProvider, NodeStats, StatsServer, DEFAULT_STATS_PORT};
pub use routing::{InterfaceType, PathEntry, PathTable, PathTableConfig, RoutingMetrics};
pub use testnet::{TestnetServer, TestnetTransport, TransportError, DEFAULT_SERVER, SERVERS};

#[cfg(feature = "esp32")]
pub use network::WifiNetwork;

#[cfg(not(feature = "esp32"))]
pub use network::HostNetwork;

/// Initialize ESP-IDF for tests. Uses Once to ensure it only runs once.
/// Also connects to WiFi if credentials are stored in NVS.
/// This is a no-op on non-ESP32 targets.
#[cfg(feature = "esp32")]
pub fn ensure_esp_initialized() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        esp_idf_svc::sys::link_patches();
        esp_idf_svc::log::EspLogger::initialize_default();

        // Try to connect to WiFi if credentials are stored
        // This enables network tests to run automatically
        try_connect_wifi();
    });
}

/// Global flag indicating whether WiFi is connected.
/// Used by network tests to skip if network isn't available.
#[cfg(feature = "esp32")]
static WIFI_CONNECTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Check if WiFi is connected (ESP32 only).
/// On host, always returns true (system network is available).
///
/// Uses Acquire ordering to ensure visibility of WiFi initialization side effects.
#[cfg(feature = "esp32")]
pub fn is_wifi_connected() -> bool {
    WIFI_CONNECTED.load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(not(feature = "esp32"))]
pub fn is_wifi_connected() -> bool {
    true // Host always has network
}

/// Attempt to connect to WiFi using stored NVS credentials.
/// Silently does nothing if no credentials are stored or connection fails.
#[cfg(feature = "esp32")]
fn try_connect_wifi() {
    use log::info;
    use std::sync::atomic::Ordering;

    // Check for stored WiFi config
    let config = match wifi::init_nvs() {
        Ok(nvs) => wifi::load_wifi_config(&nvs),
        Err(_) => return,
    };

    let config = match config {
        Some(c) => c,
        None => {
            info!("No WiFi config in NVS - network tests will be skipped");
            return;
        }
    };

    info!("Found WiFi config for '{}', connecting...", config.ssid);

    // Get peripherals for WiFi
    let peripherals = match esp_idf_svc::hal::peripherals::Peripherals::take() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Could not take peripherals for WiFi: {:?}", e);
            return;
        }
    };

    let sysloop = match esp_idf_svc::eventloop::EspSystemEventLoop::take() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Could not take event loop for WiFi: {:?}", e);
            return;
        }
    };

    // Connect to WiFi
    match wifi::WifiManager::new(peripherals.modem, sysloop) {
        Ok(mut wifi_manager) => {
            match wifi_manager.connect(&config) {
                Ok(ip) => {
                    info!("WiFi connected (IP: {}) - network tests enabled", ip);
                    // Release ordering ensures WiFi initialization is visible to other threads
                    WIFI_CONNECTED.store(true, Ordering::Release);
                }
                Err(e) => log::warn!("WiFi connection failed: {:?}", e),
            }
            // INTENTIONAL LEAK: Keep wifi_manager alive for the duration of the process.
            // This is necessary because:
            // 1. WifiManager holds references to singleton peripherals (modem, event loop)
            // 2. These peripherals can only be taken once via Peripherals::take()
            // 3. Dropping WifiManager would invalidate the WiFi connection
            // 4. For test runners, WiFi must stay connected throughout all tests
            // The ~20KB is acceptable for test infrastructure on 512KB SRAM.
            std::mem::forget(wifi_manager);
        }
        Err(e) => log::warn!("Could not create WiFi manager: {:?}", e),
    }
}

#[cfg(not(feature = "esp32"))]
pub fn ensure_esp_initialized() {
    // No-op on host
}

/// Get the default NVS partition, taking it only once.
///
/// This function ensures `EspNvsPartition::take()` is called at most once
/// across the entire application. Multiple modules can safely call this
/// to get a clone of the partition handle.
#[cfg(feature = "esp32")]
pub fn get_nvs_default_partition(
) -> Result<esp_idf_svc::nvs::EspNvsPartition<esp_idf_svc::nvs::NvsDefault>, esp_idf_sys::EspError>
{
    use esp_idf_svc::nvs::{EspNvsPartition, NvsDefault};
    use std::sync::OnceLock;

    // Store Result to handle fallible initialization with stable OnceLock
    static NVS_PARTITION: OnceLock<Result<EspNvsPartition<NvsDefault>, esp_idf_sys::EspError>> =
        OnceLock::new();

    NVS_PARTITION
        .get_or_init(|| EspNvsPartition::<NvsDefault>::take())
        .clone()
}

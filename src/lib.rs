//! Reticulum-rs ESP32 firmware library.
//!
//! This library contains platform-independent components that can be tested
//! on the host machine without ESP32 hardware.

pub mod announce;
pub mod ble;
pub mod chat;
pub mod config;
pub mod lora;
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
pub use network::{NetworkError, NetworkProvider, NodeStats, StatsServer, DEFAULT_STATS_PORT};
pub use routing::{InterfaceType, PathEntry, PathTable, PathTableConfig, RoutingMetrics};
pub use testnet::{TestnetServer, TestnetTransport, TransportError, DEFAULT_SERVER, SERVERS};

#[cfg(feature = "esp32")]
pub use network::WifiNetwork;

#[cfg(not(feature = "esp32"))]
pub use network::HostNetwork;

/// Initialize ESP-IDF for tests. Uses Once to ensure it only runs once.
/// This is a no-op on non-ESP32 targets.
#[cfg(feature = "esp32")]
pub fn ensure_esp_initialized() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        esp_idf_svc::sys::link_patches();
        esp_idf_svc::log::EspLogger::initialize_default();
    });
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

//! BLE GATT service for node configuration.
//!
//! This module provides a BLE service that allows configuring the transport node
//! from a phone app like nRF Connect.
//!
//! # GATT Service Structure
//!
//! ```text
//! Service: Node Configuration
//! ├── Status (Read, Notify) - Current connection status
//! ├── SSID (Read, Write) - WiFi network name
//! ├── Password (Write) - WiFi network password
//! └── Command (Write) - Control commands (connect/disconnect/clear)
//! ```
//!
//! # Future Extensions
//!
//! This service will be extended to support additional configuration:
//! - Testnet server selection
//! - Announce filtering (gateway mode)
//! - LoRa region selection
//!
//! See [docs/future-work.md](../../docs/future-work.md) for details.
//!
//! # Security Considerations
//!
//! - **Link-layer encryption**: BLE communication uses link-layer encryption after
//!   pairing, but WiFi credentials are transmitted as plaintext at the application
//!   layer within that encrypted channel.
//!
//! - **Pairing environment**: Configuration should be performed in a physically
//!   secure environment. An attacker within BLE range (~10m) could potentially
//!   intercept credentials during initial setup before pairing is complete.
//!
//! - **Production hardening**: For production deployments, consider:
//!   - Enabling BLE pairing with PIN authentication
//!   - Adding out-of-band authentication (e.g., QR code with shared secret)
//!   - Disabling BLE advertising after successful WiFi connection
//!   - Implementing a physical button requirement to enable configuration mode

use super::wifi::{
    ConfigCommand, WifiConfig, WifiStatus, MAX_PASSWORD_LEN, MAX_SSID_LEN,
};
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{uuid128, BLEDevice, BLEServer, NimbleProperties};
use std::sync::{Arc, Mutex};
use zeroize::Zeroize;

/// Custom UUID for Node Configuration Service.
/// Generated: https://www.uuidgenerator.net/
const CONFIG_SERVICE_UUID: BleUuid = uuid128!("12345678-1234-5678-1234-56789abcdef0");

/// UUID for Status characteristic.
const STATUS_CHAR_UUID: BleUuid = uuid128!("12345678-1234-5678-1234-56789abcdef1");

/// UUID for SSID characteristic.
const SSID_CHAR_UUID: BleUuid = uuid128!("12345678-1234-5678-1234-56789abcdef2");

/// UUID for Password characteristic.
const PASSWORD_CHAR_UUID: BleUuid = uuid128!("12345678-1234-5678-1234-56789abcdef3");

/// UUID for Command characteristic.
const COMMAND_CHAR_UUID: BleUuid = uuid128!("12345678-1234-5678-1234-56789abcdef4");

/// BLE advertisement name when unconfigured.
const DEVICE_NAME_UNCONFIGURED: &str = "Reticulum-Unconfigured";

/// BLE advertisement name when configured.
const DEVICE_NAME_CONFIGURED: &str = "Reticulum-Node";

/// BLE GATT service for WiFi configuration.
///
/// This will be renamed to `ConfigService` when additional configuration
/// options are added.
pub struct WifiConfigService {
    /// Current WiFi status.
    status: Arc<Mutex<WifiStatus>>,
    /// Pending SSID (written but not yet connected).
    pending_ssid: Arc<Mutex<String>>,
    /// Pending password.
    pending_password: Arc<Mutex<String>>,
    /// Pending command to execute.
    pending_command: Arc<Mutex<Option<ConfigCommand>>>,
}

impl WifiConfigService {
    /// Create and register the configuration BLE service.
    pub fn new(server: &mut BLEServer) -> Self {
        let status = Arc::new(Mutex::new(WifiStatus::Unconfigured));
        let pending_ssid = Arc::new(Mutex::new(String::new()));
        let pending_password = Arc::new(Mutex::new(String::new()));
        let pending_command = Arc::new(Mutex::new(None));

        // Create GATT service
        let service = server.create_service(CONFIG_SERVICE_UUID);

        // Status characteristic (Read + Notify)
        let status_clone = status.clone();
        let status_char = service.lock().create_characteristic(
            STATUS_CHAR_UUID,
            NimbleProperties::READ | NimbleProperties::NOTIFY,
        );
        status_char.lock().on_read(move |char, _conn| {
            let s = status_clone.lock().unwrap();
            char.set_value(s.to_ble_string().as_bytes());
        });

        // SSID characteristic (Read + Write)
        let ssid_clone = pending_ssid.clone();
        let ssid_read_clone = pending_ssid.clone();
        let ssid_char = service.lock().create_characteristic(
            SSID_CHAR_UUID,
            NimbleProperties::READ | NimbleProperties::WRITE,
        );
        ssid_char.lock().on_read(move |char, _conn| {
            let ssid = ssid_read_clone.lock().unwrap();
            char.set_value(ssid.as_bytes());
        });
        ssid_char.lock().on_write(move |args| {
            let data = args.recv_data();
            // Reject oversized data before allocating (prevents memory exhaustion)
            if data.len() > MAX_SSID_LEN {
                log::warn!("Rejected oversized SSID: {} bytes", data.len());
                return;
            }
            match String::from_utf8(data.to_vec()) {
                Ok(s) => {
                    let mut ssid = ssid_clone.lock().unwrap();
                    *ssid = s;
                }
                Err(e) => log::warn!("SSID write rejected: invalid UTF-8: {}", e),
            }
        });

        // Password characteristic (Write only)
        let password_clone = pending_password.clone();
        let password_char = service
            .lock()
            .create_characteristic(PASSWORD_CHAR_UUID, NimbleProperties::WRITE);
        password_char.lock().on_write(move |args| {
            let data = args.recv_data();
            // Reject oversized data before allocating (prevents memory exhaustion)
            if data.len() > MAX_PASSWORD_LEN {
                log::warn!("Rejected oversized password: {} bytes", data.len());
                return;
            }
            match String::from_utf8(data.to_vec()) {
                Ok(s) => {
                    let mut password = password_clone.lock().unwrap();
                    *password = s;
                }
                Err(e) => log::warn!("Password write rejected: invalid UTF-8: {}", e),
            }
        });

        // Command characteristic (Write only)
        let command_clone = pending_command.clone();
        let command_char = service
            .lock()
            .create_characteristic(COMMAND_CHAR_UUID, NimbleProperties::WRITE);
        command_char.lock().on_write(move |args| {
            if let Ok(s) = String::from_utf8(args.recv_data().to_vec()) {
                if let Ok(cmd) = s.parse::<ConfigCommand>() {
                    let mut command = command_clone.lock().unwrap();
                    *command = Some(cmd);
                }
            }
        });

        Self {
            status,
            pending_ssid,
            pending_password,
            pending_command,
        }
    }

    /// Start BLE advertising.
    pub fn start_advertising(configured: bool) {
        let device = BLEDevice::take();
        let advertising = device.get_advertising();

        let name = if configured {
            DEVICE_NAME_CONFIGURED
        } else {
            DEVICE_NAME_UNCONFIGURED
        };

        advertising
            .lock()
            .set_data(
                esp32_nimble::BLEAdvertisementData::new()
                    .name(name)
                    .add_service_uuid(CONFIG_SERVICE_UUID),
            )
            .unwrap();

        advertising.lock().start().unwrap();
    }

    /// Get current WiFi status.
    pub fn get_status(&self) -> WifiStatus {
        self.status.lock().unwrap().clone()
    }

    /// Update WiFi status and notify connected clients.
    pub fn set_status(&self, new_status: WifiStatus) {
        let mut status = self.status.lock().unwrap();
        *status = new_status;
        // TODO: Notify connected clients via characteristic notification
    }

    /// Take pending command if available.
    pub fn take_pending_command(&self) -> Option<ConfigCommand> {
        let mut cmd = self.pending_command.lock().unwrap();
        cmd.take()
    }

    /// Get pending WiFi configuration.
    ///
    /// Returns `Some(config)` if SSID has been written, `None` otherwise.
    pub fn get_pending_config(&self) -> Option<WifiConfig> {
        let ssid = self.pending_ssid.lock().unwrap();
        let password = self.pending_password.lock().unwrap();

        if ssid.is_empty() {
            return None;
        }

        WifiConfig::new(ssid.clone(), password.clone()).ok()
    }

    /// Clear pending configuration.
    ///
    /// Securely zeros the password before clearing to prevent memory leaks.
    pub fn clear_pending(&self) {
        let mut ssid = self.pending_ssid.lock().unwrap();
        let mut password = self.pending_password.lock().unwrap();
        password.zeroize(); // Zero password memory before clearing
        ssid.clear();
        password.clear();
    }
}

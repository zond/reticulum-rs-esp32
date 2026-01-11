//! BLE GATT service for WiFi configuration.
//!
//! This module provides a BLE service that allows configuring WiFi credentials
//! from a phone app like nRF Connect.
//!
//! # GATT Service Structure
//!
//! ```text
//! Service: WiFi Configuration
//! ├── Status (Read, Notify) - Current connection status
//! ├── SSID (Read, Write) - Network name
//! ├── Password (Write) - Network password
//! └── Command (Write) - Control commands (connect/disconnect/clear)
//! ```

use super::config::{ConfigCommand, WifiConfig, WifiStatus};
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{uuid128, BLEDevice, BLEServer, NimbleProperties};
use std::sync::{Arc, Mutex};

/// Custom UUID for WiFi Configuration Service.
/// Generated: https://www.uuidgenerator.net/
const WIFI_CONFIG_SERVICE_UUID: BleUuid = uuid128!("12345678-1234-5678-1234-56789abcdef0");

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
    /// Create and register the WiFi configuration BLE service.
    pub fn new(server: &mut BLEServer) -> Self {
        let status = Arc::new(Mutex::new(WifiStatus::Unconfigured));
        let pending_ssid = Arc::new(Mutex::new(String::new()));
        let pending_password = Arc::new(Mutex::new(String::new()));
        let pending_command = Arc::new(Mutex::new(None));

        // Create GATT service
        let service = server.create_service(WIFI_CONFIG_SERVICE_UUID);

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
            if let Ok(s) = String::from_utf8(args.recv_data().to_vec()) {
                let mut ssid = ssid_clone.lock().unwrap();
                *ssid = s;
            }
        });

        // Password characteristic (Write only)
        let password_clone = pending_password.clone();
        let password_char = service
            .lock()
            .create_characteristic(PASSWORD_CHAR_UUID, NimbleProperties::WRITE);
        password_char.lock().on_write(move |args| {
            if let Ok(s) = String::from_utf8(args.recv_data().to_vec()) {
                let mut password = password_clone.lock().unwrap();
                *password = s;
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
                    .add_service_uuid(WIFI_CONFIG_SERVICE_UUID),
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
    pub fn clear_pending(&self) {
        let mut ssid = self.pending_ssid.lock().unwrap();
        let mut password = self.pending_password.lock().unwrap();
        ssid.clear();
        password.clear();
    }
}

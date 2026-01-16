//! WiFi configuration utility for ESP32.
//!
//! Stores WiFi credentials to NVS for use by tests and the main application.
//!
//! Usage:
//!   WIFI_SSID="MyNetwork" WIFI_PASSWORD="secret" cargo configure-wifi
//!
//! For open networks (no password):
//!   WIFI_SSID="OpenNetwork" WIFI_PASSWORD="" cargo configure-wifi
//!
//! After running this once, the ESP32 will remember the credentials across reboots.

/// WiFi SSID - set via WIFI_SSID environment variable at compile time.
#[cfg(feature = "esp32")]
const WIFI_SSID: Option<&str> = option_env!("WIFI_SSID");

/// WiFi password - set via WIFI_PASSWORD environment variable at compile time.
/// Empty string for open networks.
#[cfg(feature = "esp32")]
const WIFI_PASSWORD: Option<&str> = option_env!("WIFI_PASSWORD");

/// Print error message and halt. On ESP32, we pause briefly then return
/// so the process terminates cleanly (espflash monitor will show the output).
#[cfg(feature = "esp32")]
fn halt_with_error(msg: &str) -> ! {
    eprintln!("\n{}", msg);
    eprintln!("\n=== Configuration failed ===\n");
    // Brief pause to ensure serial output is flushed before process exits
    std::thread::sleep(std::time::Duration::from_secs(2));
    std::process::exit(1);
}

#[cfg(feature = "esp32")]
fn main() {
    use reticulum_rs_esp32::config::{ConfigError, WifiConfig};
    use reticulum_rs_esp32::wifi::{init_nvs, save_wifi_config};

    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    println!("\n=== WiFi Configuration Utility ===\n");

    // Check for compile-time credentials
    let ssid = match WIFI_SSID {
        Some(s) if !s.is_empty() => s,
        _ => {
            halt_with_error(
                "Error: WIFI_SSID environment variable not set at compile time.\n\n\
                 Usage:\n  \
                 WIFI_SSID=\"MyNetwork\" WIFI_PASSWORD=\"secret\" cargo configure-wifi\n\n\
                 For open networks:\n  \
                 WIFI_SSID=\"OpenNetwork\" WIFI_PASSWORD=\"\" cargo configure-wifi",
            );
        }
    };

    let password = WIFI_PASSWORD.unwrap_or("");

    println!("SSID: {}", ssid);
    println!(
        "Password: {} ({} chars)",
        if password.is_empty() {
            "(none)"
        } else {
            "****"
        },
        password.len()
    );

    // Validate and create config
    let config = match WifiConfig::new(ssid.to_string(), password.to_string()) {
        Ok(config) => config,
        Err(ConfigError::SsidEmpty) => {
            halt_with_error("Error: SSID cannot be empty");
        }
        Err(ConfigError::SsidTooLong { len, max }) => {
            halt_with_error(&format!(
                "Error: SSID too long ({} bytes, max {})",
                len, max
            ));
        }
        Err(ConfigError::PasswordTooShort { len, min }) => {
            halt_with_error(&format!(
                "Error: Password too short ({} bytes, min {} for WPA)",
                len, min
            ));
        }
        Err(ConfigError::PasswordTooLong { len, max }) => {
            halt_with_error(&format!(
                "Error: Password too long ({} bytes, max {})",
                len, max
            ));
        }
        Err(e) => {
            halt_with_error(&format!("Error: {:?}", e));
        }
    };

    // Initialize NVS and save config
    match init_nvs() {
        Ok(mut nvs) => match save_wifi_config(&mut nvs, &config) {
            Ok(()) => {
                println!("\n=== WiFi configuration saved to NVS ===");
                println!("\nThe ESP32 will now use these credentials for network tests.");
                println!("Credentials persist across reboots.");
            }
            Err(e) => {
                halt_with_error(&format!("Error saving to NVS: {:?}", e));
            }
        },
        Err(e) => {
            halt_with_error(&format!("Error initializing NVS: {:?}", e));
        }
    }

    println!("\n=== Done - you can disconnect the device ===\n");

    // Brief pause to ensure serial output is visible, then exit cleanly
    std::thread::sleep(std::time::Duration::from_secs(2));
}

#[cfg(not(feature = "esp32"))]
fn main() {
    eprintln!("This binary must be built for ESP32.");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  WIFI_SSID=\"MyNetwork\" WIFI_PASSWORD=\"secret\" cargo configure-wifi");
    std::process::exit(1);
}

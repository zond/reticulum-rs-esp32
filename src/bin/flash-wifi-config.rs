//! Flash WiFi configuration to ESP32.
//!
//! This host-side utility reads WIFI_SSID and WIFI_PASSWORD environment variables,
//! builds the configure-wifi binary with those credentials embedded, and flashes it.
//!
//! Usage:
//!   WIFI_SSID="MyNetwork" WIFI_PASSWORD="secret" cargo run --bin flash-wifi-config
//!
//! Or use the cargo alias:
//!   WIFI_SSID="MyNetwork" WIFI_PASSWORD="secret" cargo configure-wifi

// This binary only runs on the host, not on ESP32
#![cfg(not(target_os = "espidf"))]

use reticulum_rs_esp32::host_utils::{find_esp32_port, flash_and_monitor};
use std::path::PathBuf;
use std::process::{exit, Command};

const CHIP: &str = "esp32";

fn main() {
    // Read credentials from environment
    let ssid = match std::env::var("WIFI_SSID") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            eprintln!("Error: WIFI_SSID environment variable not set.");
            eprintln!();
            eprintln!("Usage:");
            eprintln!("  WIFI_SSID=\"MyNetwork\" WIFI_PASSWORD=\"secret\" cargo configure-wifi");
            eprintln!();
            eprintln!("For open networks:");
            eprintln!("  WIFI_SSID=\"OpenNetwork\" WIFI_PASSWORD=\"\" cargo configure-wifi");
            exit(1);
        }
    };

    let password = std::env::var("WIFI_PASSWORD").unwrap_or_default();

    println!("=== WiFi Configuration ===\n");
    println!(
        "SSID: {}\nPassword: {}\n",
        ssid,
        if password.is_empty() {
            "(open network)"
        } else {
            "****"
        }
    );

    println!("=== Building configure-wifi for ESP32 ===\n");

    // Build with credentials embedded via environment variables
    let status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "configure-wifi",
            "--release",
            "--target",
            "xtensa-esp32-espidf",
            "--features",
            "esp32",
        ])
        .env("WIFI_SSID", &ssid)
        .env("WIFI_PASSWORD", &password)
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("\nBuild failed!");
        exit(1);
    }

    // Auto-detect ESP32 port
    let port = match find_esp32_port() {
        Some(p) => p,
        None => {
            eprintln!("\nNo ESP32 device found. Check USB connection.");
            exit(1);
        }
    };

    println!("\n=== Flashing to device ({}) ===\n", port);

    let binary_path = PathBuf::from("target/xtensa-esp32-espidf/release/configure-wifi");
    if let Err(e) = flash_and_monitor(&binary_path, &port, CHIP) {
        eprintln!("\nFlash failed: {}", e);
        exit(1);
    }
}

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

use reticulum_rs_esp32::host_utils::{
    flash_and_monitor_output, get_esp32_port, FlashError, PortResult, TerminalGuard,
};
use std::io::Write;
use std::ops::ControlFlow;
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

    // Get ESP32 port (PORT env var or auto-detect)
    let port = match get_esp32_port() {
        PortResult::Found(p) => p,
        PortResult::MultipleDevices(ports) => {
            eprintln!("\nMultiple ESP32 devices found:");
            for port in &ports {
                eprintln!("  {}", port);
            }
            eprintln!("\nSet PORT environment variable to specify which device to use.");
            exit(1);
        }
        PortResult::NotFound => {
            eprintln!("\nNo ESP32 device found. Check USB connection.");
            eprintln!("Tip: Set PORT environment variable to specify device.");
            exit(1);
        }
    };

    println!("\n=== Flashing to device ({}) ===\n", port);

    let binary_path = PathBuf::from("target/xtensa-esp32-espidf/release/configure-wifi");

    // TerminalGuard ensures terminal is reset even if we panic or exit early
    // (espflash monitor can leave terminal in raw mode)
    let _term_guard = TerminalGuard;

    // Flash and monitor until configuration completes (30 second timeout)
    // Use explicit \r\n because espflash may leave terminal in raw mode
    let result = flash_and_monitor_output(&binary_path, &port, CHIP, 30, |line| {
        print!("{}\r\n", line);
        let _ = std::io::stdout().flush();
        if line.contains("=== Done") {
            ControlFlow::Break(Ok(()))
        } else if line.contains("Error:") || line.contains("=== Configuration failed ===") {
            ControlFlow::Break(Err(FlashError::CommandFailed(
                "Device reported error".to_string(),
            )))
        } else {
            ControlFlow::Continue(())
        }
    });

    print!("\r\n");
    match result {
        Ok(()) => {
            print!("=== WiFi configuration complete ===\r\n");
        }
        Err(e) => {
            eprint!("Configuration failed: {}\r\n", e);
            exit(1);
        }
    }
}

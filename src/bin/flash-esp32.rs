//! Flash node binary to ESP32-S3 hardware.
//!
//! Usage: cargo run --bin flash-esp32

// This binary only runs on the host, not on ESP32
#![cfg(not(target_os = "espidf"))]

use reticulum_rs_esp32::host_utils::{flash_and_monitor, get_esp32_port, PortResult};
use std::path::PathBuf;
use std::process::{exit, Command};

const CHIP: &str = "esp32s3";

fn main() {
    println!("=== Building node for ESP32-S3 ===\n");

    let status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "node",
            "--release",
            "--target",
            "xtensa-esp32s3-espidf",
            "--features",
            "esp32",
        ])
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

    let binary_path = PathBuf::from("target/xtensa-esp32s3-espidf/release/node");
    if let Err(e) = flash_and_monitor(&binary_path, &port, CHIP) {
        eprintln!("\nFlash failed: {}", e);
        exit(1);
    }
}

//! Host-side utilities for ESP32 development.
//!
//! This module provides utilities for flashing and monitoring ESP32 devices
//! from the host machine. Only available when not building for ESP32.

use std::path::Path;
use std::process::{Command, Stdio};

/// Find an ESP32 serial port by scanning common device patterns.
///
/// Returns the first matching port, or None if no device is found.
pub fn find_esp32_port() -> Option<String> {
    let patterns = [
        "/dev/cu.usbserial-*",
        "/dev/cu.wchusbserial*",
        "/dev/cu.SLAB_USBtoUART*",
        "/dev/ttyUSB*",
        "/dev/ttyACM*",
    ];

    for pattern in patterns {
        if let Ok(paths) = glob::glob(pattern) {
            if let Some(path) = paths.flatten().next() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// List available serial ports for debugging.
pub fn list_available_ports() -> Vec<String> {
    let mut available_ports = Vec::new();

    // macOS patterns
    if let Ok(paths) = glob::glob("/dev/cu.*") {
        available_ports.extend(paths.flatten().map(|p| p.to_string_lossy().to_string()));
    }

    // Linux USB serial patterns (more specific than /dev/tty* to avoid iterating
    // over hundreds of virtual terminals)
    for pattern in ["/dev/ttyUSB*", "/dev/ttyACM*"] {
        if let Ok(paths) = glob::glob(pattern) {
            available_ports.extend(paths.flatten().map(|p| p.to_string_lossy().to_string()));
        }
    }

    available_ports
}

/// Flash a binary to an ESP32 device.
///
/// Shows progress bar by using inherited stdout.
///
/// # Arguments
/// * `binary_path` - Path to the binary to flash
/// * `port` - Serial port (e.g., "/dev/cu.usbserial-XXX")
/// * `chip` - Chip type (e.g., "esp32", "esp32s3")
pub fn flash_binary(binary_path: &Path, port: &str, chip: &str) -> Result<(), FlashError> {
    let status = Command::new("espflash")
        .args([
            "flash",
            "--non-interactive",
            "--chip",
            chip,
            "--port",
            port,
            &binary_path.to_string_lossy(),
        ])
        .status()
        .map_err(|e| FlashError::CommandFailed(e.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(FlashError::FlashFailed)
    }
}

/// Flash a binary with monitoring (for utilities that need to see output).
///
/// Shows progress bar, then enters monitor mode.
pub fn flash_and_monitor(binary_path: &Path, port: &str, chip: &str) -> Result<(), FlashError> {
    let status = Command::new("espflash")
        .args([
            "flash",
            "--monitor",
            "--chip",
            chip,
            "--port",
            port,
            &binary_path.to_string_lossy(),
        ])
        .status()
        .map_err(|e| FlashError::CommandFailed(e.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(FlashError::FlashFailed)
    }
}

/// Start monitoring an ESP32 device, returning the process for output capture.
///
/// Returns a child process with piped stdout for reading output.
pub fn start_monitor(port: &str) -> Result<std::process::Child, FlashError> {
    Command::new("espflash")
        .args(["monitor", "--non-interactive", "--port", port])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| FlashError::CommandFailed(e.to_string()))
}

/// Find QEMU for ESP32 emulation.
///
/// Searches in Espressif tools directory and PATH.
pub fn find_qemu() -> Option<std::path::PathBuf> {
    // Try to find QEMU in Espressif tools directory (any version)
    if let Ok(home) = std::env::var("HOME") {
        let pattern = format!(
            "{}/.espressif/tools/qemu-xtensa/*/qemu/bin/qemu-system-xtensa",
            home
        );
        if let Ok(paths) = glob::glob(&pattern) {
            // Get the most recent version by sorting paths (version numbers sort naturally)
            let mut candidates: Vec<_> = paths.flatten().collect();
            candidates.sort();
            if let Some(path) = candidates.pop() {
                return Some(path);
            }
        }
    }

    // Fallback: check if QEMU is in PATH
    if let Ok(output) = Command::new("which").arg("qemu-system-xtensa").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(std::path::PathBuf::from(path));
            }
        }
    }

    None
}

/// Errors that can occur during flash operations.
#[derive(Debug)]
pub enum FlashError {
    /// Failed to execute espflash command.
    CommandFailed(String),
    /// Flash operation failed.
    FlashFailed,
    /// No device found.
    NoDevice,
}

impl std::fmt::Display for FlashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed(e) => write!(f, "command failed: {}", e),
            Self::FlashFailed => write!(f, "flash operation failed"),
            Self::NoDevice => write!(f, "no ESP32 device found"),
        }
    }
}

impl std::error::Error for FlashError {}

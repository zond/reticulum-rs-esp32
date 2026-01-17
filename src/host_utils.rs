//! Host-side utilities for ESP32 development.
//!
//! This module provides utilities for flashing and monitoring ESP32 devices
//! from the host machine. Only available when not building for ESP32.

use log::debug;
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// Maximum lines to process before aborting (protects against infinite output).
const MAX_OUTPUT_LINES: usize = 100_000;

/// RAII guard to ensure child process is always cleaned up.
pub struct ProcessGuard(pub Child);

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// RAII guard to reset terminal state on drop.
///
/// espflash monitor can leave the terminal in raw mode. This guard
/// ensures `stty sane` is called when the guard is dropped.
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = Command::new("stty").arg("sane").status();
    }
}

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

/// Monitor serial output with a custom handler.
///
/// Reads lines from stdout and passes each to the callback.
/// Returns when the callback signals completion or timeout is reached.
///
/// Uses binary-safe reading (read_until) and handles ESP32's \r\n line endings.
///
/// # Arguments
/// * `stdout` - Stdout to read from (e.g., from `start_monitor` or QEMU process)
/// * `timeout_secs` - Maximum seconds to wait
/// * `on_line` - Callback for each line; returns `Continue` to keep reading,
///   or `Break(result)` to stop with success/failure
pub fn monitor_output<R, F, E>(stdout: R, timeout_secs: u64, mut on_line: F) -> Result<(), E>
where
    R: std::io::Read,
    F: FnMut(&str) -> std::ops::ControlFlow<Result<(), E>>,
    E: From<String>,
{
    use std::io::BufRead;
    use std::io::BufReader;
    use std::ops::ControlFlow;
    use std::time::{Duration, Instant};

    let mut reader = BufReader::new(stdout);
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let mut buf = Vec::new();
    let mut line_count: usize = 0;

    loop {
        buf.clear();

        if start.elapsed() > timeout {
            return Err(E::from(format!("Timeout after {} seconds", timeout_secs)));
        }

        // Protect against infinite output
        if line_count >= MAX_OUTPUT_LINES {
            return Err(E::from(format!(
                "Output exceeded {} lines, possible infinite loop",
                MAX_OUTPUT_LINES
            )));
        }

        // Read until newline, handling binary data gracefully
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => return Err(E::from("Monitor ended without completion".to_string())),
            Ok(_) => {}
            Err(e) => {
                // Log I/O errors but continue (may be transient)
                if e.kind() != std::io::ErrorKind::Interrupted {
                    debug!("I/O error reading output: {}", e);
                }
                continue;
            }
        }

        // Convert to string, replacing invalid UTF-8 with replacement char.
        // Strip \r characters (ESP32 uses \r\n line endings).
        let line = String::from_utf8_lossy(&buf).trim().replace('\r', "");

        // Skip empty lines and garbage
        if line.is_empty() || line.chars().all(|c| c == '\u{FFFD}' || c.is_control()) {
            continue;
        }

        line_count += 1;

        if let ControlFlow::Break(result) = on_line(&line) {
            return result;
        }
    }
}

/// Flash a binary and monitor output with a custom handler.
///
/// Flashes the binary, then monitors serial output. Each line is passed to
/// the `on_line` callback which decides whether to continue or stop.
///
/// # Arguments
/// * `binary_path` - Path to the binary to flash
/// * `port` - Serial port
/// * `chip` - Chip type
/// * `timeout_secs` - Maximum seconds to wait
/// * `on_line` - Callback for each line; returns `Continue` to keep reading,
///   or `Break(result)` to stop with success/failure
///
/// # Example
/// ```ignore
/// flash_and_monitor_output(&binary, &port, "esp32", 30, |line| {
///     println!("{}", line);
///     if line.contains("=== Done") {
///         ControlFlow::Break(Ok(()))
///     } else if line.contains("Error:") {
///         ControlFlow::Break(Err(FlashError::CommandFailed("error".into())))
///     } else {
///         ControlFlow::Continue(())
///     }
/// })?;
/// ```
pub fn flash_and_monitor_output<F>(
    binary_path: &Path,
    port: &str,
    chip: &str,
    timeout_secs: u64,
    on_line: F,
) -> Result<(), FlashError>
where
    F: FnMut(&str) -> std::ops::ControlFlow<Result<(), FlashError>>,
{
    // Flash the binary
    flash_binary(binary_path, port, chip)?;

    // Start monitoring - ProcessGuard ensures cleanup on any exit path
    let mut process = start_monitor(port)?;
    let stdout = process
        .stdout
        .take()
        .ok_or_else(|| FlashError::CommandFailed("Failed to capture stdout".to_string()))?;

    let _guard = ProcessGuard(process);
    monitor_output(stdout, timeout_secs, on_line)
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

impl From<String> for FlashError {
    fn from(s: String) -> Self {
        FlashError::CommandFailed(s)
    }
}

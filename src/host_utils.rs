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
#[must_use = "ProcessGuard must be held to ensure process cleanup"]
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
#[must_use = "TerminalGuard must be held to ensure terminal restoration"]
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = Command::new("stty").arg("sane").status();
    }
}

/// Result from port detection.
pub enum PortResult {
    /// Single port found or PORT env var specified.
    Found(String),
    /// Multiple ports found, user must specify PORT env var.
    MultipleDevices(Vec<String>),
    /// No devices found.
    NotFound,
}

/// Get ESP32 serial port from PORT environment variable or auto-detect.
///
/// - If PORT env var is set, uses that port directly
/// - If exactly one device is found, uses it automatically
/// - If multiple devices are found, returns error requiring PORT to be set
/// - If no devices are found, returns NotFound
pub fn get_esp32_port() -> PortResult {
    // Check PORT environment variable first
    if let Ok(port) = std::env::var("PORT") {
        if !port.is_empty() {
            return PortResult::Found(port);
        }
    }

    // Find all available ESP32 ports
    let mut ports = find_all_esp32_ports();

    match ports.len() {
        0 => PortResult::NotFound,
        1 => PortResult::Found(ports.swap_remove(0)),
        _ => PortResult::MultipleDevices(ports),
    }
}

/// Find all ESP32 serial ports by scanning common device patterns.
fn find_all_esp32_ports() -> Vec<String> {
    let patterns = [
        "/dev/cu.usbserial-*",
        "/dev/cu.wchusbserial*",
        "/dev/cu.SLAB_USBtoUART*",
        "/dev/ttyUSB*",
        "/dev/ttyACM*",
    ];

    let mut ports = Vec::new();
    for pattern in patterns {
        if let Ok(paths) = glob::glob(pattern) {
            for path in paths.flatten() {
                ports.push(path.to_string_lossy().into_owned());
            }
        }
    }
    ports
}

/// Find an ESP32 serial port by scanning common device patterns.
///
/// Returns the first matching port, or None if no device is found.
/// Prefer `get_esp32_port()` which also checks the PORT environment variable.
pub fn find_esp32_port() -> Option<String> {
    find_all_esp32_ports().into_iter().next()
}

/// List available serial ports for debugging.
pub fn list_available_ports() -> Vec<String> {
    let mut available_ports = Vec::new();

    // macOS patterns
    if let Ok(paths) = glob::glob("/dev/cu.*") {
        available_ports.extend(paths.flatten().map(|p| p.to_string_lossy().into_owned()));
    }

    // Linux USB serial patterns (more specific than /dev/tty* to avoid iterating
    // over hundreds of virtual terminals)
    for pattern in ["/dev/ttyUSB*", "/dev/ttyACM*"] {
        if let Ok(paths) = glob::glob(pattern) {
            available_ports.extend(paths.flatten().map(|p| p.to_string_lossy().into_owned()));
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

/// Flash a binary with interactive monitoring.
///
/// Flashes the binary, then monitors serial output indefinitely until
/// the user presses Ctrl+C. All output is printed to stdout.
pub fn flash_and_monitor(binary_path: &Path, port: &str, chip: &str) -> Result<(), FlashError> {
    use std::io::Write;

    flash_binary(binary_path, port, chip)?;

    println!("\n=== Monitoring (Ctrl+C to exit) ===\n");

    // Monitor with long timeout - user exits with Ctrl+C.
    // 1 hour timeout prevents resource leaks if user forgets about it.
    // Ignore errors since any exit (Ctrl+C, timeout, pipe close) is expected.
    let _ = flash_and_monitor_impl(port, 3600, |line| {
        print!("{}\r\n", line);
        let _ = std::io::stdout().flush();
        std::ops::ControlFlow::Continue(())
    });

    Ok(())
}

/// Internal monitor implementation used by both flash_and_monitor and flash_and_monitor_output.
fn flash_and_monitor_impl<F>(port: &str, timeout_secs: u64, on_line: F) -> Result<(), FlashError>
where
    F: FnMut(&str) -> std::ops::ControlFlow<Result<(), FlashError>>,
{
    let process = start_monitor(port)?;
    // Create guard IMMEDIATELY to ensure cleanup even if stdout.take() fails
    let mut guard = ProcessGuard(process);

    let stdout = guard
        .0
        .stdout
        .take()
        .ok_or_else(|| FlashError::CommandFailed("Failed to capture stdout".to_string()))?;

    monitor_output(stdout, timeout_secs, on_line)
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
    let mut consecutive_errors: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;

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
            Ok(_) => {
                consecutive_errors = 0; // Reset on success
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue; // Interrupted is transient, retry immediately
                }

                consecutive_errors += 1;
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    return Err(E::from(format!(
                        "Too many consecutive I/O errors ({}), last: {}",
                        MAX_CONSECUTIVE_ERRORS, e
                    )));
                }

                debug!(
                    "I/O error reading output ({}/{}): {}",
                    consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                );
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
    // Flash the binary (espflash skips unchanged segments automatically)
    flash_binary(binary_path, port, chip)?;

    // Monitor with callback
    flash_and_monitor_impl(port, timeout_secs, on_line)
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

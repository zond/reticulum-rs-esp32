//! LoRa interface adapter for reticulum-rs transport.
//!
//! This module bridges the SX1262 radio driver with the reticulum-rs transport layer.
//! It implements the `Interface` trait and provides an async worker that:
//! - Polls for incoming packets and forwards them to the transport
//! - Receives outgoing packets from the transport and transmits them
//!
//! # Half-Duplex Management
//!
//! LoRa is half-duplex - the radio cannot transmit and receive simultaneously.
//! This worker prioritizes TX over RX since we control when to transmit but
//! cannot control when packets arrive.
//!
//! # Blocking Bridge Pattern
//!
//! The SX1262 radio driver uses blocking SPI calls. To avoid blocking the
//! async runtime, each radio operation runs in `tokio::task::spawn_blocking`.
//!
//! # Usage
//!
//! ```ignore
//! use reticulum_rs_esp32::lora::{LoRaRadio, LoRaInterface, Region};
//!
//! // Create and initialize the radio
//! let mut radio = LoRaRadio::new(spi, sclk, mosi, miso, cs, reset, busy, dio1, Region::Eu868)?;
//! radio.init()?;
//!
//! // Wrap it in the interface adapter
//! let lora_iface = LoRaInterface::new(radio);
//!
//! // Register with transport
//! transport.iface_manager().lock().await.spawn(lora_iface, LoRaInterface::spawn);
//! ```

use super::config::LORA_MTU;
use super::radio::{LoRaRadio, ReceivedPacket};
use log::{debug, error, info, warn};
use reticulum::iface::{Interface, InterfaceContext, RxMessage};
use reticulum::packet::Packet;
use std::time::Duration;

/// LoRa receive timeout per poll (ms).
///
/// Short timeout to allow checking for TX requests. The radio alternates
/// between RX polling and TX handling since LoRa is half-duplex.
const RX_TIMEOUT_MS: u32 = 50;

/// Delay after an error before retrying (ms).
const ERROR_BACKOFF_MS: u64 = 100;

/// LoRa interface adapter for reticulum-rs transport.
///
/// This struct wraps the low-level radio driver and adapts it to the
/// reticulum-rs interface system. It handles:
/// - Async polling for received packets
/// - Transmitting packets from the transport layer
/// - Error handling and logging
pub struct LoRaInterface<'d> {
    radio: LoRaRadio<'d>,
}

impl<'d> LoRaInterface<'d> {
    /// Create a new LoRa interface from an initialized radio.
    ///
    /// The radio must be initialized before passing to this function.
    pub fn new(radio: LoRaRadio<'d>) -> Self {
        Self { radio }
    }

    /// Get a reference to the underlying radio.
    pub fn radio(&self) -> &LoRaRadio<'d> {
        &self.radio
    }

    /// Get a mutable reference to the underlying radio.
    pub fn radio_mut(&mut self) -> &mut LoRaRadio<'d> {
        &mut self.radio
    }

    /// Spawn the LoRa interface worker task.
    ///
    /// This function runs the main interface loop that:
    /// 1. Checks for outgoing packets from the transport
    /// 2. Transmits any pending outgoing packets
    /// 3. Polls for incoming packets from the radio
    /// 4. Forwards received packets to the transport
    ///
    /// The loop runs until cancellation is signaled.
    pub async fn spawn(context: InterfaceContext<LoRaInterface<'d>>)
    where
        'd: 'static,
    {
        let iface_address = context.channel.address;
        info!("LoRa interface started: {:?}", iface_address);

        // Split the channel to get ownership of sender/receiver
        let (rx_channel, mut tx_channel) = context.channel.split();

        loop {
            // Check for cancellation
            if context.cancel.is_cancelled() {
                info!("LoRa interface shutting down");
                break;
            }

            // Priority 1: Handle TX (we control when to transmit)
            if let Ok(tx_msg) = tx_channel.try_recv() {
                let packet = tx_msg.packet;
                // TODO: Consider pre-allocated buffer to reduce heap fragmentation
                let data = packet.raw().to_vec();
                debug!("LoRa TX: {} bytes", data.len());

                let inner = context.inner.clone();
                let result = tokio::task::spawn_blocking(move || {
                    // Handle poisoned mutex - recover by taking the inner value
                    let mut iface = match inner.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!("LoRa interface mutex was poisoned, recovering");
                            poisoned.into_inner()
                        }
                    };
                    iface.radio.transmit(&data)
                })
                .await;

                match result {
                    Ok(Ok(())) => {
                        debug!("LoRa TX complete");
                    }
                    Ok(Err(e)) => {
                        warn!("LoRa TX error: {}", e);
                    }
                    Err(e) => {
                        // Task panicked - this is serious but we try to continue
                        // TODO: Consider radio reinitialization on repeated panics
                        error!("LoRa TX task panicked: {}", e);
                    }
                }

                // Yield to allow other async tasks to run before checking for more TX
                tokio::task::yield_now().await;
                continue;
            }

            // Priority 2: Poll for RX
            let rx_result = {
                let inner = context.inner.clone();
                tokio::task::spawn_blocking(move || {
                    // Handle poisoned mutex
                    let mut iface = match inner.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!("LoRa interface mutex was poisoned, recovering");
                            poisoned.into_inner()
                        }
                    };
                    iface.radio.receive(RX_TIMEOUT_MS)
                })
                .await
            };

            match rx_result {
                Ok(Ok(Some(received))) => {
                    if let Err(e) = handle_rx_packet(&rx_channel, iface_address, received).await {
                        warn!("Failed to forward RX packet: {}", e);
                    }
                }
                Ok(Ok(None)) => {
                    // No packet received, normal operation
                }
                Ok(Err(e)) => {
                    warn!("LoRa RX error: {}", e);
                    // Brief delay on error to avoid tight loop
                    tokio::time::sleep(Duration::from_millis(ERROR_BACKOFF_MS)).await;
                }
                Err(e) => {
                    // Task panicked
                    error!("LoRa RX task panicked: {}", e);
                    tokio::time::sleep(Duration::from_millis(ERROR_BACKOFF_MS)).await;
                }
            }
        }

        info!("LoRa interface stopped");
    }
}

impl<'d> Interface for LoRaInterface<'d> {
    fn mtu() -> usize {
        LORA_MTU
    }
}

/// Handle a received packet by forwarding it to the transport.
async fn handle_rx_packet(
    rx_channel: &reticulum::iface::InterfaceRxSender,
    iface_address: reticulum::hash::AddressHash,
    received: ReceivedPacket,
) -> Result<(), String> {
    debug!(
        "LoRa RX: {} bytes, RSSI {} dBm, SNR {} dB",
        received.data.len(),
        received.rssi,
        received.snr
    );

    // Validate packet size before parsing
    if received.data.is_empty() {
        return Err("Empty packet received".to_string());
    }
    if received.data.len() > LORA_MTU {
        return Err(format!(
            "Packet too large: {} bytes (max {})",
            received.data.len(),
            LORA_MTU
        ));
    }

    // Parse the raw bytes into a packet
    let packet = Packet::try_from(received.data.as_slice())
        .map_err(|e| format!("Invalid packet: {:?}", e))?;

    // Forward to transport
    let rx_msg = RxMessage {
        address: iface_address,
        packet,
    };

    rx_channel
        .send(rx_msg)
        .await
        .map_err(|e| format!("Failed to send to transport: {}", e))
}

// Note: Tests for this module require ESP32 hardware and are validated
// through integration testing on actual devices.

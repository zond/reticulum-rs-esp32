//! SX1262 radio driver for ESP32.
//!
//! This module provides the LoRa radio interface for the LILYGO T3-S3 board,
//! using the `sx1262` crate for the radio protocol.
//!
//! # Pin Configuration (LILYGO T3-S3)
//!
//! | Signal | GPIO | Notes |
//! |--------|------|-------|
//! | SPI MOSI | 11 | Master Out Slave In |
//! | SPI MISO | 13 | Master In Slave Out |
//! | SPI CLK | 12 | SPI Clock |
//! | NSS (CS) | 10 | Chip Select |
//! | RESET | 5 | Radio Reset |
//! | BUSY | 4 | Radio Busy Status |
//! | DIO1 | 1 | Interrupt |

use super::config::{
    Region, BANDWIDTH_HZ, LORA_MTU, LOW_DATA_RATE_OPTIMIZE, PREAMBLE_LENGTH, SPREADING_FACTOR,
    TX_POWER,
};
use super::csma::{Csma, CsmaConfig, CsmaResult};
use super::{calculate_airtime_us, DutyCycleLimiter, LoRaParams};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{Gpio1, Gpio10, Gpio4, Gpio5, Input, Output, PinDriver};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::spi::config::Config as SpiConfig;
use esp_idf_hal::spi::config::DriverConfig;
use esp_idf_hal::spi::{SpiDeviceDriver, SpiDriver, SPI2};
use esp_idf_hal::units::FromValueType;
use log::{debug, info, warn};
use regiface::{Command, NoParameters, ToByteArray};
use std::fmt;
use std::time::Duration;
use sx1262::{
    ClearIrqStatus, Device, DeviceSelect, DioIrqConfig, GetIrqStatus, GetPacketStatus,
    GetRxBufferStatus, IrqMask, PaConfig, PacketParams, PacketType, RampTime, RfFrequencyConfig,
    RxMode, SetDioIrqParams, SetPaConfig, SetPacketParams, SetPacketType, SetRfFrequency, SetRx,
    SetStandby, SetTx, SetTxParams, StandbyConfig, Timeout, TxParams,
};

/// Maximum time to wait for radio to become ready (ms).
const BUSY_TIMEOUT_MS: u32 = 1000;

/// Maximum time to wait for TX completion (seconds).
const TX_TIMEOUT_SECS: u64 = 5;

/// RSSI settling time after entering RX mode (ms).
/// Per SX1262 community reports, 5ms is reliable for accurate RSSI readings.
const RSSI_SETTLING_MS: u32 = 5;

// SX1262 LoRa modulation parameter values (per datasheet Table 13-47, 13-48).
// We use raw bytes because the sx1262 crate's LoRaBandwidth enum has incorrect values.
const LORA_SF7: u8 = 0x07;
const LORA_BW_125_KHZ: u8 = 0x04;
const LORA_CR_4_5: u8 = 0x01;

/// Raw LoRa modulation parameters (bypasses sx1262 crate's broken bandwidth enum).
///
/// Format: [SF, BW, CR, LowDataRateOpt, 0, 0, 0, 0]
#[derive(Debug, Clone)]
struct RawLoRaModParams([u8; 8]);

impl RawLoRaModParams {
    fn new(sf: u8, bw: u8, cr: u8, low_data_rate_opt: bool) -> Self {
        Self([sf, bw, cr, low_data_rate_opt as u8, 0, 0, 0, 0])
    }
}

impl ToByteArray for RawLoRaModParams {
    type Error = core::convert::Infallible;
    type Array = [u8; 8];

    fn to_bytes(self) -> Result<Self::Array, Self::Error> {
        Ok(self.0)
    }
}

/// Raw SetModulationParams command (opcode 0x8B).
#[derive(Debug, Clone)]
struct RawSetModulationParams {
    params: RawLoRaModParams,
}

impl Command for RawSetModulationParams {
    type IdType = u8;
    type CommandParameters = RawLoRaModParams;
    type ResponseParameters = NoParameters;

    fn id() -> Self::IdType {
        0x8B
    }

    fn invoking_parameters(self) -> Self::CommandParameters {
        self.params
    }
}

/// Raw GetRssiInst response (instantaneous RSSI reading).
#[derive(Debug, Clone, Default)]
struct RssiInstResponse {
    rssi: u8,
}

impl regiface::FromByteArray for RssiInstResponse {
    type Error = core::convert::Infallible;
    type Array = [u8; 2]; // status byte + RSSI byte

    fn from_bytes(bytes: Self::Array) -> Result<Self, Self::Error> {
        Ok(Self { rssi: bytes[1] })
    }
}

/// Raw GetRssiInst command (opcode 0x15).
#[derive(Debug, Clone, Default)]
struct GetRssiInst;

impl Command for GetRssiInst {
    type IdType = u8;
    type CommandParameters = NoParameters;
    type ResponseParameters = RssiInstResponse;

    fn id() -> Self::IdType {
        0x15
    }

    fn invoking_parameters(self) -> Self::CommandParameters {
        Default::default()
    }
}

/// LoRa radio interface.
pub struct LoRaRadio<'d> {
    device: Device<SpiDeviceDriver<'d, SpiDriver<'d>>>,
    reset: PinDriver<'d, Gpio5, Output>,
    busy: PinDriver<'d, Gpio4, Input>,
    // DIO1 stored for future interrupt-driven RX/TX (currently using polling).
    // TODO: Implement interrupt handler for better power efficiency.
    #[allow(dead_code)]
    dio1: PinDriver<'d, Gpio1, Input>,
    region: Region,
    duty_cycle: DutyCycleLimiter,
    csma: Csma,
    initialized: bool,
}

impl<'d> LoRaRadio<'d> {
    /// Create a new LoRa radio instance for the given region.
    ///
    /// This initializes the SPI bus and GPIO pins but does not configure the radio.
    /// Call [`init`] to configure the radio for operation.
    pub fn new(
        spi: impl Peripheral<P = SPI2> + 'd,
        sclk: impl Peripheral<P = esp_idf_hal::gpio::Gpio12> + 'd,
        mosi: impl Peripheral<P = esp_idf_hal::gpio::Gpio11> + 'd,
        miso: impl Peripheral<P = esp_idf_hal::gpio::Gpio13> + 'd,
        cs: impl Peripheral<P = Gpio10> + 'd,
        reset: impl Peripheral<P = Gpio5> + 'd,
        busy: impl Peripheral<P = Gpio4> + 'd,
        dio1: impl Peripheral<P = Gpio1> + 'd,
        region: Region,
    ) -> Result<Self, RadioError> {
        // Configure SPI (SX1262 supports up to 16MHz, use conservative 2MHz)
        let spi_config = SpiConfig::new().baudrate(2.MHz().into());
        let driver_config = DriverConfig::new();

        let spi_driver =
            SpiDriver::new(spi, sclk, mosi, Some(miso), &driver_config).map_err(RadioError::Spi)?;

        let spi_device =
            SpiDeviceDriver::new(spi_driver, Some(cs), &spi_config).map_err(RadioError::Spi)?;

        let device = Device::new(spi_device);

        let reset_pin = PinDriver::output(reset).map_err(RadioError::Gpio)?;
        let busy_pin = PinDriver::input(busy).map_err(RadioError::Gpio)?;
        let dio1_pin = PinDriver::input(dio1).map_err(RadioError::Gpio)?;

        let duty_cycle = region.duty_cycle_limiter();
        let csma = Csma::new(CsmaConfig::default());

        Ok(Self {
            device,
            reset: reset_pin,
            busy: busy_pin,
            dio1: dio1_pin,
            region,
            duty_cycle,
            csma,
            initialized: false,
        })
    }

    /// Initialize the radio.
    ///
    /// This resets the radio and configures it for LoRa operation.
    pub fn init(&mut self) -> Result<(), RadioError> {
        info!("Initializing SX1262 radio for {:?}", self.region);

        self.hardware_reset()?;
        self.wait_busy()?;

        // Set standby mode
        self.device
            .execute_command(SetStandby {
                config: StandbyConfig::Rc,
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Set packet type to LoRa
        self.device
            .execute_command(SetPacketType {
                packet_type: PacketType::LoRa,
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Set RF frequency
        self.device
            .execute_command(SetRfFrequency {
                config: RfFrequencyConfig {
                    frequency: self.region.frequency(),
                },
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Set modulation parameters using raw bytes (sx1262 crate has incorrect bandwidth enum)
        self.device
            .execute_command(RawSetModulationParams {
                params: RawLoRaModParams::new(
                    LORA_SF7,
                    LORA_BW_125_KHZ,
                    LORA_CR_4_5,
                    LOW_DATA_RATE_OPTIMIZE,
                ),
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Set packet parameters (raw bytes for LoRa mode)
        // Format: [preamble_hi, preamble_lo, header_type, payload_len, crc_on, invert_iq, 0, 0, 0]
        let packet_params = build_lora_packet_params(PREAMBLE_LENGTH, LORA_MTU as u8, true, false);
        self.device
            .execute_command(SetPacketParams {
                params: packet_params,
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Configure PA for SX1262 (+22dBm capable)
        self.device
            .execute_command(SetPaConfig {
                config: PaConfig {
                    duty_cycle: 0x04,
                    hp_max: 0x07,
                    device_sel: DeviceSelect::Sx1262,
                    pa_lut: 0x01,
                },
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Set TX parameters
        self.device
            .execute_command(SetTxParams {
                params: TxParams {
                    power: TX_POWER,
                    ramp_time: RampTime::Micros200,
                },
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Configure DIO1 for TX done and RX done interrupts
        self.configure_irq()?;

        // Seed CSMA RNG from hardware random number generator
        let seed = unsafe { esp_idf_sys::esp_random() };
        self.csma.seed(seed);

        self.initialized = true;
        info!(
            "SX1262 initialized: {} MHz, SF{}, {}kHz, {} dBm",
            self.region.frequency() / 1_000_000,
            SPREADING_FACTOR,
            BANDWIDTH_HZ / 1000,
            TX_POWER
        );

        Ok(())
    }

    /// Configure IRQ settings.
    fn configure_irq(&mut self) -> Result<(), RadioError> {
        let irq_mask = IrqMask::TX_DONE | IrqMask::RX_DONE | IrqMask::TIMEOUT;
        self.device
            .execute_command(SetDioIrqParams {
                config: DioIrqConfig {
                    irq_mask,
                    dio1_mask: irq_mask,
                    dio2_mask: IrqMask::empty(),
                    dio3_mask: IrqMask::empty(),
                },
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;
        Ok(())
    }

    /// Hardware reset the radio.
    fn hardware_reset(&mut self) -> Result<(), RadioError> {
        debug!("Resetting radio");
        self.reset.set_low().map_err(RadioError::Gpio)?;
        FreeRtos::delay_ms(1);
        self.reset.set_high().map_err(RadioError::Gpio)?;
        FreeRtos::delay_ms(10);
        Ok(())
    }

    /// Wait for the radio to be ready (BUSY pin low).
    fn wait_busy(&self) -> Result<(), RadioError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(BUSY_TIMEOUT_MS as u64);

        while self.busy.is_high() {
            if start.elapsed() > timeout {
                return Err(RadioError::Timeout);
            }
            FreeRtos::delay_ms(1);
        }

        Ok(())
    }

    /// Read instantaneous RSSI from the radio.
    ///
    /// Returns RSSI in dBm. Used for CSMA/CA channel sensing.
    fn get_rssi(&mut self) -> Result<i16, RadioError> {
        self.wait_busy()?;
        let response = self
            .device
            .execute_command(GetRssiInst)
            .map_err(RadioError::Command)?;
        // RSSI = -raw_value/2 dBm (per SX1262 datasheet)
        Ok(-(response.rssi as i16) / 2)
    }

    /// Transmit a packet.
    ///
    /// Uses CSMA/CA to avoid collisions on the shared frequency.
    /// Returns an error if the channel is busy after max retries or duty cycle is exceeded.
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), RadioError> {
        if !self.initialized {
            return Err(RadioError::NotInitialized);
        }

        if data.is_empty() {
            return Err(RadioError::EmptyPacket);
        }

        if data.len() > LORA_MTU {
            return Err(RadioError::PacketTooLarge {
                size: data.len(),
                max: LORA_MTU,
            });
        }

        // Calculate airtime for duty cycle check (done after CSMA succeeds)
        let airtime_us = calculate_airtime_us(data.len(), &LoRaParams::default());

        // CSMA/CA: check channel before transmitting
        // Enter RX mode for channel sensing (stays in RX during backoff to detect activity)
        self.device
            .execute_command(SetRx {
                mode: RxMode::Continuous,
            })
            .map_err(|e| {
                self.csma.reset();
                RadioError::Command(e)
            })?;

        loop {
            FreeRtos::delay_ms(RSSI_SETTLING_MS);

            let rssi = match self.get_rssi() {
                Ok(r) => r,
                Err(e) => {
                    self.csma.reset();
                    let _ = self.device.execute_command(SetStandby {
                        config: StandbyConfig::Rc,
                    });
                    return Err(e);
                }
            };

            match self.csma.try_access(rssi) {
                CsmaResult::Transmit => {
                    debug!(
                        "Channel clear (RSSI {} dBm), transmitting {} bytes",
                        rssi,
                        data.len()
                    );
                    break;
                }
                CsmaResult::Wait { ms } => {
                    debug!(
                        "Channel busy (RSSI {} dBm), waiting {}ms (retry {})",
                        rssi,
                        ms,
                        self.csma.retries()
                    );
                    // Stay in RX mode during backoff to detect channel activity
                    FreeRtos::delay_ms(ms);
                }
                CsmaResult::GiveUp => {
                    warn!(
                        "Channel busy after {} retries, dropping packet",
                        self.csma.retries()
                    );
                    self.csma.reset();
                    let _ = self.device.execute_command(SetStandby {
                        config: StandbyConfig::Rc,
                    });
                    return Err(RadioError::ChannelBusy);
                }
            }
        }

        // CSMA succeeded - now return to standby and check duty cycle
        self.csma.reset();
        self.device
            .execute_command(SetStandby {
                config: StandbyConfig::Rc,
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Check duty cycle after CSMA succeeds (avoids consuming budget on failed CSMA)
        if !self.duty_cycle.try_consume(airtime_us) {
            warn!(
                "Duty cycle exceeded: {:.1}% remaining",
                self.duty_cycle.remaining_percent()
            );
            return Err(RadioError::DutyCycleExceeded);
        }

        // Set packet length for this transmission
        let packet_params =
            build_lora_packet_params(PREAMBLE_LENGTH, data.len() as u8, true, false);
        self.device
            .execute_command(SetPacketParams {
                params: packet_params,
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Write data to buffer
        self.device
            .write_buffer(0, data)
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Clear IRQ flags
        self.device
            .execute_command(ClearIrqStatus {
                irq_mask: IrqMask::all(),
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Start TX
        self.device
            .execute_command(SetTx {
                timeout: Timeout(0),
            })
            .map_err(RadioError::Command)?;

        // Wait for TX done
        self.wait_tx_done()?;

        // Return to standby
        self.device
            .execute_command(SetStandby {
                config: StandbyConfig::Rc,
            })
            .map_err(RadioError::Command)?;

        Ok(())
    }

    /// Receive a packet (blocking with timeout).
    ///
    /// Returns `Ok(None)` if no packet received within timeout.
    pub fn receive(&mut self, timeout_ms: u32) -> Result<Option<ReceivedPacket>, RadioError> {
        if !self.initialized {
            return Err(RadioError::NotInitialized);
        }

        // Clear IRQ flags
        self.device
            .execute_command(ClearIrqStatus {
                irq_mask: IrqMask::all(),
            })
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        // Set RX mode with timeout
        let rx_mode = if timeout_ms == 0 {
            RxMode::Continuous
        } else {
            // Timeout is in units of 15.625us
            let timeout_units = (timeout_ms as u64 * 1000) / 15625;
            RxMode::Timed(timeout_units.min(0xFFFFFF) as u32)
        };

        self.device
            .execute_command(SetRx { mode: rx_mode })
            .map_err(RadioError::Command)?;

        // Wait for RX done or timeout
        let irq = self.wait_rx_done(timeout_ms + 100)?;

        if irq.contains(IrqMask::TIMEOUT) {
            return Ok(None);
        }

        if !irq.contains(IrqMask::RX_DONE) {
            return Ok(None);
        }

        // Get RX buffer status
        let status = self
            .device
            .execute_command(GetRxBufferStatus)
            .map_err(RadioError::Command)?;
        self.wait_busy()?;

        let payload_len = status.buffer_status.payload_length as usize;
        let buffer_offset = status.buffer_status.buffer_pointer;

        if payload_len == 0 || payload_len > LORA_MTU {
            return Ok(None);
        }

        // Read payload
        let mut data = vec![0u8; payload_len];
        self.device
            .read_buffer(buffer_offset, &mut data)
            .map_err(RadioError::Command)?;

        // Get packet status (RSSI, SNR)
        let packet_status = self
            .device
            .execute_command(GetPacketStatus)
            .map_err(RadioError::Command)?;

        // Return to standby
        self.device
            .execute_command(SetStandby {
                config: StandbyConfig::Rc,
            })
            .map_err(RadioError::Command)?;

        // LoRa mode: status[0]=RSSI (-val/2 dBm), status[1]=SNR (val/4 dB)
        let rssi = -(packet_status.packet_status.status[0] as i16) / 2;
        let snr = (packet_status.packet_status.status[1] as i8) / 4;

        Ok(Some(ReceivedPacket { data, rssi, snr }))
    }

    /// Wait for TX to complete.
    fn wait_tx_done(&mut self) -> Result<(), RadioError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(TX_TIMEOUT_SECS);

        loop {
            self.wait_busy()?;
            let irq = self
                .device
                .execute_command(GetIrqStatus)
                .map_err(RadioError::Command)?;

            if irq.irq_mask.contains(IrqMask::TX_DONE) {
                self.device
                    .execute_command(ClearIrqStatus {
                        irq_mask: IrqMask::all(),
                    })
                    .map_err(RadioError::Command)?;
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(RadioError::Timeout);
            }

            FreeRtos::delay_ms(1);
        }
    }

    /// Wait for RX to complete.
    fn wait_rx_done(&mut self, timeout_ms: u32) -> Result<IrqMask, RadioError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms as u64);

        loop {
            self.wait_busy()?;
            let irq = self
                .device
                .execute_command(GetIrqStatus)
                .map_err(RadioError::Command)?;

            if irq.irq_mask.contains(IrqMask::RX_DONE) || irq.irq_mask.contains(IrqMask::TIMEOUT) {
                self.device
                    .execute_command(ClearIrqStatus {
                        irq_mask: IrqMask::all(),
                    })
                    .map_err(RadioError::Command)?;
                return Ok(irq.irq_mask);
            }

            if start.elapsed() > timeout {
                return Err(RadioError::Timeout);
            }

            FreeRtos::delay_ms(1);
        }
    }

    /// Get duty cycle remaining percentage.
    pub fn duty_cycle_remaining(&mut self) -> f32 {
        self.duty_cycle.remaining_percent()
    }

    /// Get the region this radio is configured for.
    pub fn region(&self) -> Region {
        self.region
    }
}

/// Build LoRa packet parameters as raw bytes.
///
/// Format: [preamble_hi, preamble_lo, header_type, payload_len, crc_on, invert_iq, 0, 0, 0]
fn build_lora_packet_params(
    preamble: u16,
    payload_len: u8,
    crc_enabled: bool,
    invert_iq: bool,
) -> PacketParams {
    PacketParams {
        params: [
            (preamble >> 8) as u8,   // Preamble high byte
            (preamble & 0xFF) as u8, // Preamble low byte
            0x00,                    // Header type: 0=explicit, 1=implicit
            payload_len,
            if crc_enabled { 0x01 } else { 0x00 },
            if invert_iq { 0x01 } else { 0x00 },
            0,
            0,
            0,
        ],
    }
}

/// A received LoRa packet.
#[derive(Debug, Clone)]
pub struct ReceivedPacket {
    /// Packet payload.
    pub data: Vec<u8>,
    /// RSSI in dBm.
    pub rssi: i16,
    /// SNR in dB.
    pub snr: i8,
}

/// Radio errors.
#[derive(Debug)]
pub enum RadioError {
    /// SPI communication error.
    Spi(esp_idf_sys::EspError),
    /// GPIO error.
    Gpio(esp_idf_sys::EspError),
    /// Command execution error.
    Command(sx1262::Error),
    /// Radio not initialized.
    NotInitialized,
    /// Radio busy timeout.
    Timeout,
    /// Duty cycle limit exceeded.
    DutyCycleExceeded,
    /// Channel busy after CSMA/CA retries.
    ChannelBusy,
    /// Packet too large.
    PacketTooLarge { size: usize, max: usize },
    /// Empty packet.
    EmptyPacket,
}

impl fmt::Display for RadioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spi(e) => write!(f, "SPI error: {:?}", e),
            Self::Gpio(e) => write!(f, "GPIO error: {:?}", e),
            Self::Command(e) => write!(f, "command error: {:?}", e),
            Self::NotInitialized => write!(f, "radio not initialized"),
            Self::Timeout => write!(f, "radio timeout"),
            Self::DutyCycleExceeded => write!(f, "duty cycle exceeded"),
            Self::ChannelBusy => write!(f, "channel busy"),
            Self::PacketTooLarge { size, max } => {
                write!(f, "packet too large: {} bytes (max {})", size, max)
            }
            Self::EmptyPacket => write!(f, "empty packet"),
        }
    }
}

impl std::error::Error for RadioError {}

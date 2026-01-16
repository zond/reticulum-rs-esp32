# Task: Interrupt-Driven LoRa Radio

## Goal

Convert the LoRa radio driver from polling to interrupt-driven operation for better power efficiency. Currently, `wait_tx_done()` and `wait_rx_done()` poll the radio every 1ms. The SX1262 already pulses DIO1 on TX_DONE/RX_DONE/TIMEOUT events - we just need to listen.

## Current State

**File:** `src/lora/radio.rs`

- DIO1 (GPIO1) is stored but unused (line 137-140, marked `#[allow(dead_code)]`)
- SX1262 configured to pulse DIO1 on events (lines 295-310)
- `wait_tx_done()` polls every 1ms with `FreeRtos::delay_ms(1)` (lines 574-600)
- `wait_rx_done()` polls every 1ms with `FreeRtos::delay_ms(1)` (lines 603-629)

## Implementation Approach

Use esp-idf-hal's `Notification` API for ISR-to-task signaling:
- Configure DIO1 for positive edge interrupt
- ISR sends notification when DIO1 fires
- Wait functions block on notification instead of polling

### Why Notification (not async)?

The current architecture uses `spawn_blocking` in `iface.rs` - the radio already runs in a blocking context. `Notification::wait()` integrates with FreeRTOS scheduler (task sleeps efficiently). Converting to async would require restructuring the entire call chain.

## Changes Required

### 1. Add Fields to `LoRaRadio` Struct

```rust
use esp_idf_hal::gpio::InterruptType;
use esp_idf_hal::task::notification::Notification;

pub struct LoRaRadio<'d> {
    // ... existing fields ...
    dio1: PinDriver<'d, Gpio1, Input>,  // Remove #[allow(dead_code)]

    // NEW: Interrupt notification
    irq_notification: Notification,
}
```

### 2. Initialize Interrupt in `init()`

After `configure_irq()`:

```rust
// Configure DIO1 for positive edge interrupt
self.dio1.set_interrupt_type(InterruptType::PosEdge)?;

// Create notification and subscribe to interrupts
let notifier = self.irq_notification.notifier();
unsafe {
    self.dio1.subscribe(move || {
        // ISR context: just signal, read flags later
        notifier.notify_and_yield(NonZeroU32::new(1).unwrap());
    })?;
}
self.dio1.enable_interrupt()?;
```

### 3. Replace `wait_tx_done()`

```rust
fn wait_tx_done(&mut self) -> Result<(), RadioError> {
    let timeout_ticks = TX_TIMEOUT_SECS * 1000;

    loop {
        match self.irq_notification.wait(timeout_ticks as u32) {
            Some(_) => {
                self.dio1.enable_interrupt()?;  // Re-enable for next event
                self.wait_busy()?;
                let irq = self.device.execute_command(GetIrqStatus)?;

                if irq.irq_mask.contains(IrqMask::TX_DONE) {
                    self.device.execute_command(ClearIrqStatus { irq_mask: IrqMask::all() })?;
                    return Ok(());
                }
                // Not TX_DONE, continue waiting
            }
            None => return Err(RadioError::Timeout),
        }
    }
}
```

### 4. Replace `wait_rx_done()`

```rust
fn wait_rx_done(&mut self, timeout_ms: u32) -> Result<IrqMask, RadioError> {
    let timeout_ticks = timeout_ms.saturating_add(100);  // Margin for overhead

    loop {
        match self.irq_notification.wait(timeout_ticks) {
            Some(_) => {
                self.dio1.enable_interrupt()?;
                self.wait_busy()?;
                let irq = self.device.execute_command(GetIrqStatus)?;

                if irq.irq_mask.contains(IrqMask::RX_DONE)
                    || irq.irq_mask.contains(IrqMask::TIMEOUT) {
                    self.device.execute_command(ClearIrqStatus { irq_mask: IrqMask::all() })?;
                    return Ok(irq.irq_mask);
                }
            }
            None => return Err(RadioError::Timeout),
        }
    }
}
```

### 5. Add Drop Implementation

```rust
impl<'d> Drop for LoRaRadio<'d> {
    fn drop(&mut self) {
        let _ = self.dio1.disable_interrupt();
        let _ = self.dio1.unsubscribe();
    }
}
```

## Files to Modify

| File | Changes |
|------|---------|
| `src/lora/radio.rs` | Add Notification field, interrupt setup, replace wait functions, add Drop |
| `src/lora/mod.rs` | May need new error variant re-export |
| `docs/future-work.md` | Mark interrupt-driven radio as resolved |

## Edge Cases

1. **Interrupt before wait**: Notification is sticky - wait returns immediately (correct behavior)
2. **Multiple rapid interrupts**: Notifications coalesce; code reads actual IRQ flags (correct)
3. **CSMA RX interrupts**: CSMA doesn't wait on notification, unaffected
4. **Interrupt during SPI**: DIO1 is independent of SPI; `wait_busy()` before reading IRQ handles this

## Verification

### QEMU Testing

QEMU doesn't emulate the SX1262 radio, so interrupt behavior cannot be tested there. The code should compile and the radio module should gracefully handle the emulated environment.

```bash
cargo test-qemu  # Verify no regression in other tests
```

### Hardware Testing

Requires actual ESP32 with LoRa radio:

1. **TX completion**: Transmit packet, verify completion time is faster than polling interval
2. **RX timeout**: Put in RX mode, verify timeout works correctly
3. **Rapid TX/RX**: Run continuous loop, verify no missed interrupts
4. **Power measurement**: Verify CPU sleeps during wait (oscilloscope/power monitor)

```bash
cargo test-esp32  # Run on real hardware
```

### Manual Smoke Test

```bash
cargo flash-esp32  # Flash and monitor
# Observe LoRa TX/RX in node output
# Verify no timeout errors or missed packets
```

## Implementation Order

1. Add `Notification` field and imports
2. Add interrupt setup in `init()`
3. Replace `wait_tx_done()`
4. Replace `wait_rx_done()`
5. Add `Drop` implementation
6. Remove `#[allow(dead_code)]` from `dio1`
7. Update docs/future-work.md
8. Test on hardware

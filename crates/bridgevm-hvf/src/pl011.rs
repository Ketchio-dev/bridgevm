//! Minimal PL011 UART model — captures guest/firmware serial output.
//!
//! On the `virt` platform the UART lives at [`crate::machine::UART`]
//! (`0x0900_0000`). This models just enough for a guest or the EDK2 firmware to
//! emit characters: writes to `UARTDR` are captured into an output buffer, and
//! reads of `UARTFR` report "transmit ready" so the writer never spins. Having
//! serial output is the prerequisite for observing every later bring-up step
//! (firmware progress, Linux/Windows boot messages).

/// PL011 register offsets (subset).
const UARTDR: u64 = 0x000; // data register
const UARTFR: u64 = 0x018; // flag register
const UARTFR_TXFE: u64 = 1 << 7; // transmit FIFO empty

/// A modelled PL011 UART.
#[derive(Debug, Default)]
pub struct Pl011 {
    output: Vec<u8>,
}

impl Pl011 {
    pub fn new() -> Self {
        Self::default()
    }

    /// MMIO read within the UART window. `UARTFR` reports transmit-empty so a
    /// writer never blocks; other registers read as zero.
    pub fn mmio_read(&self, offset: u64, _size: u8) -> u64 {
        match offset {
            UARTFR => UARTFR_TXFE,
            _ => 0,
        }
    }

    /// MMIO write within the UART window. A byte written to `UARTDR` is emitted.
    pub fn mmio_write(&mut self, offset: u64, _size: u8, value: u64) {
        if offset == UARTDR {
            self.output.push(value as u8);
        }
    }

    /// Bytes emitted so far.
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Take and clear the emitted bytes.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_writes_are_captured() {
        let mut uart = Pl011::new();
        for b in b"HI\n" {
            uart.mmio_write(UARTDR, 1, u64::from(*b));
        }
        assert_eq!(uart.output(), b"HI\n");
    }

    #[test]
    fn flag_register_reports_transmit_ready() {
        let uart = Pl011::new();
        // TXFE set, TXFF (bit 5) clear -> firmware proceeds to write.
        assert_eq!(uart.mmio_read(UARTFR, 4) & UARTFR_TXFE, UARTFR_TXFE);
        assert_eq!(uart.mmio_read(UARTFR, 4) & (1 << 5), 0);
    }

    #[test]
    fn take_output_clears() {
        let mut uart = Pl011::new();
        uart.mmio_write(UARTDR, 1, u64::from(b'X'));
        assert_eq!(uart.take_output(), b"X");
        assert!(uart.output().is_empty());
    }
}

//! Minimal PL011 UART register model for firmware and Windows serial clients.
//!
//! Independently defined from Arm DDI 0183G chapter 3, this models only the
//! behavior BridgeVM exposes: byte-oriented I/O, idle FIFO flags, writable
//! configuration readback, and the PrimeCell identification registers.

use std::collections::VecDeque;

const UARTDR: u64 = 0x000;
const UARTRSR: u64 = 0x004;
const UARTFR: u64 = 0x018;
const UARTILPR: u64 = 0x020;
const UARTIBRD: u64 = 0x024;
const UARTFBRD: u64 = 0x028;
const UARTLCR_H: u64 = 0x02c;
const UARTCR: u64 = 0x030;
const UARTIFLS: u64 = 0x034;
const UARTIMSC: u64 = 0x038;
const UARTRIS: u64 = 0x03c;
const UARTMIS: u64 = 0x040;
const UARTICR: u64 = 0x044;
const UARTDMACR: u64 = 0x048;

const UARTFR_RXFE: u64 = 1 << 4;
const UARTFR_TXFE: u64 = 1 << 7;

/// Identification bytes from Arm DDI 0183G, table 3-1. Peripheral ID 2 uses
/// revision 1, yielding `0x14`; the PrimeCell signature is `0xB105_F00D`.
const fn identification_byte(offset: u64) -> Option<u64> {
    match offset {
        0x0fe0 => Some(0x11),
        0x0fe4 => Some(0x10),
        0x0fe8 => Some(0x14),
        0x0fec => Some(0x00),
        0x0ff0 => Some(0x0d),
        0x0ff4 => Some(0xf0),
        0x0ff8 => Some(0x05),
        0x0ffc => Some(0xb1),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct WritableRegisters {
    ilpr: u8,
    ibrd: u16,
    fbrd: u8,
    lcr_h: u8,
    cr: u16,
    ifls: u8,
    imsc: u16,
    dmacr: u8,
}

impl WritableRegisters {
    fn read(&self, offset: u64) -> Option<u64> {
        let value = match offset {
            UARTILPR => u64::from(self.ilpr),
            UARTIBRD => u64::from(self.ibrd),
            UARTFBRD => u64::from(self.fbrd),
            UARTLCR_H => u64::from(self.lcr_h),
            UARTCR => u64::from(self.cr),
            UARTIFLS => u64::from(self.ifls),
            UARTIMSC => u64::from(self.imsc),
            UARTDMACR => u64::from(self.dmacr),
            _ => return None,
        };
        Some(value)
    }

    fn write(&mut self, offset: u64, value: u64) -> bool {
        match offset {
            UARTILPR => self.ilpr = value as u8,
            UARTIBRD => self.ibrd = value as u16,
            UARTFBRD => self.fbrd = (value & 0x3f) as u8,
            UARTLCR_H => self.lcr_h = value as u8,
            UARTCR => self.cr = value as u16,
            UARTIFLS => self.ifls = (value & 0x3f) as u8,
            UARTIMSC => self.imsc = (value & 0x07ff) as u16,
            UARTDMACR => self.dmacr = (value & 0x07) as u8,
            _ => return false,
        }
        true
    }
}

/// Interrupt generation, DMA, modem pins, and baud timing are outside this
/// bounded subset; their status remains inactive when configuration is retained.
#[derive(Debug, Default)]
pub struct Pl011 {
    tx: Vec<u8>,
    rx: VecDeque<u8>,
    registers: WritableRegisters,
    trace: bool,
}

impl Pl011 {
    pub fn new() -> Self {
        Self {
            trace: std::env::var_os("BRIDGEVM_TRACE_PL011").is_some(),
            ..Self::default()
        }
    }

    pub fn push_input(&mut self, bytes: &[u8]) {
        if self.trace && !bytes.is_empty() {
            eprintln!(
                "pl011: RX inject {} byte(s) (queue now {}) first=0x{:02x}",
                bytes.len(),
                self.rx.len() + bytes.len(),
                bytes[0]
            );
        }
        self.rx.extend(bytes);
    }

    pub fn input_len(&self) -> usize {
        self.rx.len()
    }

    pub fn mmio_read(&mut self, offset: u64, _size: u8) -> u64 {
        let had_rx = !self.rx.is_empty();
        let value = match offset {
            UARTDR => u64::from(self.rx.pop_front().unwrap_or(0)),
            UARTFR => UARTFR_TXFE | if self.rx.is_empty() { UARTFR_RXFE } else { 0 },
            UARTRSR | UARTRIS | UARTMIS => 0,
            _ => self
                .registers
                .read(offset)
                .or_else(|| identification_byte(offset))
                .unwrap_or(0),
        };
        self.trace_read(offset, value, had_rx);
        value
    }

    fn trace_read(&self, offset: u64, value: u64, had_rx: bool) {
        if !self.trace {
            return;
        }
        if offset == UARTDR {
            if had_rx {
                eprintln!("pl011: RX read off=0x000 -> 0x{value:02x}");
            }
        } else if offset == UARTFR {
            if had_rx {
                eprintln!("pl011: read  off=0x018 (FR, RX pending) -> 0x{value:x}");
            }
        } else {
            eprintln!("pl011: read  off=0x{offset:03x} -> 0x{value:x}");
        }
    }

    pub fn mmio_write(&mut self, offset: u64, _size: u8, value: u64) {
        if self.trace && offset != UARTDR {
            eprintln!("pl011: write off=0x{offset:03x} <- 0x{value:x}");
        }
        match offset {
            UARTDR => self.tx.push(value as u8),
            UARTRSR | UARTICR => {}
            _ => {
                self.registers.write(offset, value);
            }
        }
    }

    pub fn output(&self) -> &[u8] {
        &self.tx
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_register_moves_bytes_in_both_directions() {
        let mut uart = Pl011::new();
        uart.mmio_write(UARTDR, 1, u64::from(b'H'));
        uart.mmio_write(UARTDR, 1, u64::from(b'I'));
        assert_eq!(uart.output(), b"HI");

        uart.push_input(b" A");
        assert_eq!(uart.input_len(), 2);
        assert_eq!(uart.mmio_read(UARTDR, 1), u64::from(b' '));
        assert_eq!(uart.mmio_read(UARTDR, 1), u64::from(b'A'));
    }

    #[test]
    fn flag_register_describes_the_modelled_fifos() {
        let mut uart = Pl011::new();
        assert_eq!(uart.mmio_read(UARTFR, 4), UARTFR_TXFE | UARTFR_RXFE);
        uart.push_input(b"x");
        assert_eq!(uart.mmio_read(UARTFR, 4), UARTFR_TXFE);
        assert_eq!(uart.mmio_read(UARTDR, 1), u64::from(b'x'));
        assert_eq!(uart.mmio_read(UARTFR, 4), UARTFR_TXFE | UARTFR_RXFE);
    }

    #[test]
    fn identification_window_matches_arm_ddi_0183g() {
        let mut uart = Pl011::new();
        for (offset, expected) in [
            (0x0fe0, 0x11),
            (0x0fe4, 0x10),
            (0x0fe8, 0x14),
            (0x0fec, 0x00),
            (0x0ff0, 0x0d),
            (0x0ff4, 0xf0),
            (0x0ff8, 0x05),
            (0x0ffc, 0xb1),
        ] {
            assert_eq!(uart.mmio_read(offset, 4), expected);
        }
    }

    #[test]
    fn writable_registers_apply_their_architected_widths() {
        let mut uart = Pl011::new();
        for (offset, value, expected) in [
            (UARTILPR, 0x1ff, 0xff),
            (UARTIBRD, 0x1ffff, 0xffff),
            (UARTFBRD, 0xff, 0x3f),
            (UARTLCR_H, 0x1ff, 0xff),
            (UARTCR, 0x1ffff, 0xffff),
            (UARTIFLS, 0xff, 0x3f),
            (UARTIMSC, 0xffff, 0x07ff),
            (UARTDMACR, 0xff, 0x07),
        ] {
            uart.mmio_write(offset, 4, value);
            assert_eq!(uart.mmio_read(offset, 4), expected);
        }
    }

    #[test]
    fn take_output_clears_the_transmit_buffer() {
        let mut uart = Pl011::new();
        uart.mmio_write(UARTDR, 1, u64::from(b'X'));
        assert_eq!(uart.take_output(), b"X");
        assert!(uart.output().is_empty());
    }
}

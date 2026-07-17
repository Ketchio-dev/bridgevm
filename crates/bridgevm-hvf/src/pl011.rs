//! PL011 UART model — captures guest/firmware serial output and presents the
//! register surface Windows KDCOM needs to bind the port.
//!
//! On the `virt` platform the UART lives at [`crate::machine::UART`]
//! (`0x0900_0000`). The minimal path (firmware/Linux/Windows console) only needs
//! `UARTDR` writes captured and `UARTFR` reporting an idle FIFO. Windows kernel
//! debugging over serial (KDCOM on the ARM PL011 named by our ACPI DBG2 table)
//! needs more: `kdcom.dll` runs the PrimeCell identification sequence — it reads
//! the Peripheral/PrimeCell ID registers (`0xFE0..=0xFFC`) to confirm the part
//! is a PL011, and it programs the control/line/baud registers and reads them
//! back. With those registers reading as zero the debugger transport silently
//! declines the port (KDCOM emits nothing), which is exactly the "guest sends
//! only the firmware banner, then silence" wall. Modelling the ID registers with
//! the standard ARM values and letting the writable config registers read back
//! what was written makes KDCOM accept and drive the port.

use std::collections::VecDeque;

/// PL011 register offsets.
const UARTDR: u64 = 0x000; // data register
const UARTRSR: u64 = 0x004; // receive status / error clear
const UARTFR: u64 = 0x018; // flag register
const UARTILPR: u64 = 0x020; // IrDA low-power counter
const UARTIBRD: u64 = 0x024; // integer baud rate divisor
const UARTFBRD: u64 = 0x028; // fractional baud rate divisor
const UARTLCR_H: u64 = 0x02C; // line control
const UARTCR: u64 = 0x030; // control
const UARTIFLS: u64 = 0x034; // interrupt FIFO level select
const UARTIMSC: u64 = 0x038; // interrupt mask set/clear
const UARTRIS: u64 = 0x03C; // raw interrupt status (read-only)
const UARTMIS: u64 = 0x040; // masked interrupt status (read-only)
const UARTICR: u64 = 0x044; // interrupt clear (write-only)
const UARTDMACR: u64 = 0x048; // DMA control

const UARTFR_RXFE: u64 = 1 << 4; // receive FIFO empty
const UARTFR_TXFE: u64 = 1 << 7; // transmit FIFO empty
const UARTFR_IDLE: u64 = UARTFR_TXFE | UARTFR_RXFE;

/// PrimeCell / Peripheral ID registers (`0xFE0..=0xFFC`), read-only. These are
/// the standard ARM PL011 identification bytes (matching the QEMU model): the
/// PL011 part number `0x0011`, designer ARM, plus the fixed PrimeCell tag
/// `0xB105_F00D` split across `PCellID0..3`. Windows KDCOM reads these to
/// recognise the debug UART.
const PL011_ID: [(u64, u64); 8] = [
    (0xFE0, 0x11), // UARTPeriphID0
    (0xFE4, 0x10), // UARTPeriphID1
    (0xFE8, 0x14), // UARTPeriphID2
    (0xFEC, 0x00), // UARTPeriphID3
    (0xFF0, 0x0D), // UARTPCellID0
    (0xFF4, 0xF0), // UARTPCellID1
    (0xFF8, 0x05), // UARTPCellID2
    (0xFFC, 0xB1), // UARTPCellID3
];

/// A modelled PL011 UART.
#[derive(Debug, Default)]
pub struct Pl011 {
    output: Vec<u8>,
    input: VecDeque<u8>,
    // Writable configuration registers, stored so KDCOM's program-then-read-back
    // sequence sees consistent values. Reset defaults are zero, matching a UART
    // the firmware has not yet configured.
    ilpr: u64,
    ibrd: u64,
    fbrd: u64,
    lcr_h: u64,
    cr: u64,
    ifls: u64,
    imsc: u64,
    dmacr: u64,
    trace: bool,
}

impl Pl011 {
    pub fn new() -> Self {
        Self {
            trace: std::env::var_os("BRIDGEVM_TRACE_PL011").is_some(),
            ..Self::default()
        }
    }

    /// Queue bytes that subsequent `UARTDR` reads will consume.
    pub fn push_input(&mut self, bytes: &[u8]) {
        if self.trace && !bytes.is_empty() {
            eprintln!(
                "pl011: RX inject {} byte(s) (queue now {}) first=0x{:02x}",
                bytes.len(),
                self.input.len() + bytes.len(),
                bytes[0]
            );
        }
        self.input.extend(bytes);
    }

    /// Number of queued input bytes not yet consumed by guest reads.
    pub fn input_len(&self) -> usize {
        self.input.len()
    }

    /// MMIO read within the UART window. `UARTFR` reports idle FIFOs so writers
    /// never block and input polling sees no pending byte; the ID registers
    /// identify the PL011 to KDCOM; the writable config registers read back what
    /// was programmed; unmodelled registers read as zero.
    pub fn mmio_read(&mut self, offset: u64, _size: u8) -> u64 {
        let rx_pending_before = !self.input.is_empty();
        let value = match offset {
            UARTDR => u64::from(self.input.pop_front().unwrap_or(0)),
            UARTFR if self.input.is_empty() => UARTFR_IDLE,
            UARTFR => UARTFR_TXFE,
            UARTRSR => 0, // no framing/overrun/parity/break errors
            UARTILPR => self.ilpr,
            UARTIBRD => self.ibrd,
            UARTFBRD => self.fbrd,
            UARTLCR_H => self.lcr_h,
            UARTCR => self.cr,
            UARTIFLS => self.ifls,
            UARTIMSC => self.imsc,
            UARTRIS | UARTMIS => 0, // polled KDCOM: no interrupts pending
            UARTDMACR => self.dmacr,
            _ => PL011_ID
                .iter()
                .find_map(|&(off, val)| (off == offset).then_some(val))
                .unwrap_or(0),
        };
        if self.trace {
            // UARTDR reads consume queued RX (e.g. a KDCOM breakin byte the
            // debugger injected): always trace them — they are rare and directly
            // reveal whether the guest polls our port for debugger input. Trace
            // UARTFR only while RX is pending (the poll that precedes an RX read)
            // and every non-DR/FR register unconditionally; a bare idle UARTFR
            // poll would flood the log so it is dropped.
            if offset == UARTDR {
                if rx_pending_before {
                    eprintln!("pl011: RX read off=0x000 -> 0x{value:02x}");
                }
            } else if offset == UARTFR {
                if rx_pending_before {
                    eprintln!("pl011: read  off=0x018 (FR, RX pending) -> 0x{value:x}");
                }
            } else {
                eprintln!("pl011: read  off=0x{offset:03x} -> 0x{value:x}");
            }
        }
        value
    }

    /// MMIO write within the UART window. A byte written to `UARTDR` is emitted;
    /// writes to the configuration registers are stored so a later read returns
    /// the programmed value.
    pub fn mmio_write(&mut self, offset: u64, _size: u8, value: u64) {
        if self.trace && offset != UARTDR {
            eprintln!("pl011: write off=0x{offset:03x} <- 0x{value:x}");
        }
        match offset {
            UARTDR => self.output.push(value as u8),
            UARTILPR => self.ilpr = value & 0xFF,
            UARTIBRD => self.ibrd = value & 0xFFFF,
            UARTFBRD => self.fbrd = value & 0x3F,
            UARTLCR_H => self.lcr_h = value & 0xFF,
            UARTCR => self.cr = value & 0xFFFF,
            UARTIFLS => self.ifls = value & 0x3F,
            UARTIMSC => self.imsc = value & 0x7FF,
            UARTDMACR => self.dmacr = value & 0x7,
            UARTRSR | UARTICR => {} // error-clear / interrupt-clear: nothing latched
            _ => {}
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
        let mut uart = Pl011::new();
        // TXFE set, TXFF (bit 5) clear -> firmware proceeds to write.
        assert_eq!(uart.mmio_read(UARTFR, 4) & UARTFR_TXFE, UARTFR_TXFE);
        assert_eq!(uart.mmio_read(UARTFR, 4) & (1 << 5), 0);
    }

    #[test]
    fn flag_register_reports_receive_empty() {
        let mut uart = Pl011::new();
        // RXFE set, RXFF (bit 6) clear -> input polling sees no pending byte.
        assert_eq!(uart.mmio_read(UARTFR, 4) & UARTFR_RXFE, UARTFR_RXFE);
        assert_eq!(uart.mmio_read(UARTFR, 4) & (1 << 6), 0);
    }

    #[test]
    fn queued_input_clears_receive_empty_and_reads_from_data_register() {
        let mut uart = Pl011::new();
        uart.push_input(b" A");
        assert_eq!(uart.mmio_read(UARTFR, 4) & UARTFR_RXFE, 0);
        assert_eq!(uart.mmio_read(UARTDR, 1), u64::from(b' '));
        assert_eq!(uart.mmio_read(UARTFR, 4) & UARTFR_RXFE, 0);
        assert_eq!(uart.mmio_read(UARTDR, 1), u64::from(b'A'));
        assert_eq!(uart.mmio_read(UARTFR, 4) & UARTFR_RXFE, UARTFR_RXFE);
    }

    #[test]
    fn take_output_clears() {
        let mut uart = Pl011::new();
        uart.mmio_write(UARTDR, 1, u64::from(b'X'));
        assert_eq!(uart.take_output(), b"X");
        assert!(uart.output().is_empty());
    }

    #[test]
    fn primecell_id_registers_identify_the_pl011() {
        // Windows KDCOM reads these to recognise the debug UART; the standard
        // ARM PL011 identification bytes (matching QEMU).
        let mut uart = Pl011::new();
        let ids = [
            (0xFE0, 0x11),
            (0xFE4, 0x10),
            (0xFE8, 0x14),
            (0xFEC, 0x00),
            (0xFF0, 0x0D),
            (0xFF4, 0xF0),
            (0xFF8, 0x05),
            (0xFFC, 0xB1),
        ];
        for (off, val) in ids {
            assert_eq!(uart.mmio_read(off, 4), val, "id reg 0x{off:03x}");
        }
    }

    #[test]
    fn control_registers_read_back_what_was_programmed() {
        // KDCOM programs the baud/line/control registers and reads them back;
        // zero-on-read (the old minimal model) made it decline the port.
        let mut uart = Pl011::new();
        uart.mmio_write(UARTCR, 4, 0x301); // UARTEN|TXE|RXE
        uart.mmio_write(UARTLCR_H, 4, 0x70); // 8n1 + FIFO enable
        uart.mmio_write(UARTIBRD, 4, 0x1A);
        uart.mmio_write(UARTFBRD, 4, 0x03);
        assert_eq!(uart.mmio_read(UARTCR, 4), 0x301);
        assert_eq!(uart.mmio_read(UARTLCR_H, 4), 0x70);
        assert_eq!(uart.mmio_read(UARTIBRD, 4), 0x1A);
        assert_eq!(uart.mmio_read(UARTFBRD, 4), 0x03);
    }
}

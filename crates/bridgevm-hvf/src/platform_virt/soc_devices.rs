//! PL011 UART and PL031 RTC register access plus host-side UART byte I/O.

use super::*;

impl VirtPlatform {
    pub(crate) fn uart_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.uart.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.uart.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    pub(crate) fn rtc_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.rtc.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.rtc.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    /// Bytes the guest/firmware has written to the UART so far.
    pub fn uart_output(&self) -> &[u8] {
        self.uart.output()
    }

    /// Drain and return everything the guest has written to the UART since the
    /// last drain. Used by the KD serial bridge to forward the guest's
    /// KDCOM/serial-debug transmit stream to a host socket; unlike
    /// `uart_output()` (a non-draining borrow the boot scanner reads) this
    /// consumes the buffer so bytes are forwarded exactly once.
    pub fn take_uart_output(&mut self) -> Vec<u8> {
        self.uart.take_output()
    }

    /// Queue bytes that the guest can read from the PL011 UART data register.
    /// Live probes use this to test firmware/loader input paths while the default
    /// platform remains an unattached, receive-empty serial backend.
    pub fn push_uart_input(&mut self, bytes: &[u8]) {
        self.uart.push_input(bytes);
    }

    /// Number of preloaded PL011 input bytes still waiting to be read.
    pub fn uart_input_len(&self) -> usize {
        self.uart.input_len()
    }
}

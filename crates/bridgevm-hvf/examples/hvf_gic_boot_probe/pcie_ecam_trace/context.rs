use std::fmt;

use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

const SERIAL_PHASE_TAIL_BYTES: usize = 160;

#[derive(Debug, Clone)]
pub(crate) struct PcieEcamOwnerContext {
    pub(crate) exit: u64,
    pub(crate) ipa: u64,
    pub(crate) esr: u64,
    pub(crate) ec: u64,
    pub(crate) srt: u32,
    pub(crate) serial_phase: PcieEcamSerialPhase,
}

impl PcieEcamOwnerContext {
    #[cfg(test)]
    pub(crate) fn unattributed(ipa: u64) -> Self {
        Self {
            exit: 0,
            ipa,
            esr: 0,
            ec: 0,
            srt: 0,
            serial_phase: PcieEcamSerialPhase::unavailable(),
        }
    }

    #[cfg(test)]
    pub(crate) fn serial_phase_from_uart(uart_output: &[u8]) -> String {
        PcieEcamSerialPhase::from_uart(uart_output).to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcieEcamSerialPhase {
    bytes: [u8; SERIAL_PHASE_TAIL_BYTES],
    len: usize,
    unavailable: bool,
}

impl PcieEcamSerialPhase {
    #[cfg(test)]
    const fn unavailable() -> Self {
        Self {
            bytes: [0; SERIAL_PHASE_TAIL_BYTES],
            len: 0,
            unavailable: true,
        }
    }

    pub(crate) fn from_uart(uart_output: &[u8]) -> Self {
        let tail_start = uart_output.len().saturating_sub(SERIAL_PHASE_TAIL_BYTES);
        let tail = &uart_output[tail_start..];
        let mut phase = Self {
            bytes: [0; SERIAL_PHASE_TAIL_BYTES],
            len: tail.len(),
            unavailable: false,
        };
        phase.bytes[..tail.len()].copy_from_slice(tail);
        phase
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl fmt::Display for PcieEcamSerialPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.unavailable {
            return f.write_str("unavailable");
        }
        if self.len == 0 {
            return f.write_str("empty");
        }
        for byte in self.as_bytes() {
            write_escaped_byte(f, *byte)?;
        }
        Ok(())
    }
}

pub(crate) struct PcieEcamAccess<'a> {
    pub(crate) pc: u64,
    pub(crate) ipa: u64,
    pub(crate) exit: u64,
    pub(crate) esr: u64,
    pub(crate) ec: u64,
    pub(crate) srt: u32,
    pub(crate) op: &'a MmioOp,
    pub(crate) outcome: &'a MmioOutcome,
    #[cfg(test)]
    pub(crate) owner_context: Option<PcieEcamOwnerContext>,
}

pub(crate) fn mmio_access_kind(op: &MmioOp) -> &'static str {
    match op {
        MmioOp::Read { .. } => "read",
        MmioOp::Write { .. } => "write",
    }
}

fn write_escaped_byte(output: &mut dyn fmt::Write, byte: u8) -> fmt::Result {
    match byte {
        b'\n' => output.write_str("\\n"),
        b'\r' => output.write_str("\\r"),
        b'\t' => output.write_str("\\t"),
        b' ' => output.write_str("_"),
        b'\\' => output.write_str("\\\\"),
        0x21..=0x7e => output.write_char(char::from(byte)),
        _ => write!(output, "\\x{byte:02x}"),
    }
}

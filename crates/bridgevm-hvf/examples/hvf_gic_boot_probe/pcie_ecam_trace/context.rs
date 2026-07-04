use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

const SERIAL_PHASE_TAIL_BYTES: usize = 160;

#[derive(Debug, Clone)]
pub(crate) struct PcieEcamOwnerContext {
    pub(crate) exit: u64,
    pub(crate) ipa: u64,
    pub(crate) esr: u64,
    pub(crate) ec: u64,
    pub(crate) srt: u32,
    pub(crate) serial_phase: String,
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
            serial_phase: "unavailable".to_string(),
        }
    }

    pub(crate) fn serial_phase_from_uart(uart_output: &[u8]) -> String {
        let tail_start = uart_output.len().saturating_sub(SERIAL_PHASE_TAIL_BYTES);
        let tail = &uart_output[tail_start..];
        if tail.is_empty() {
            return "empty".to_string();
        }
        let mut serial_phase = String::with_capacity(tail.len());
        for byte in tail {
            push_escaped_byte(&mut serial_phase, *byte);
        }
        serial_phase
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

fn push_escaped_byte(output: &mut String, byte: u8) {
    match byte {
        b'\n' => output.push_str("\\n"),
        b'\r' => output.push_str("\\r"),
        b'\t' => output.push_str("\\t"),
        b' ' => output.push('_'),
        b'\\' => output.push_str("\\\\"),
        0x21..=0x7e => output.push(char::from(byte)),
        _ => output.push_str(&format!("\\x{byte:02x}")),
    }
}

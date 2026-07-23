//! WFI/WFE instruction and wake diagnostics.

use crate::*;

pub(crate) const ARM64_WFI: u32 = 0xd503_207f;

pub(crate) const ARM64_WFE: u32 = 0xd503_205f;

pub(crate) const G011_INSN_WINDOW_BEFORE: u64 = 0x20;

pub(crate) const G011_INSN_WINDOW_LEN: usize = 0x60;

pub(crate) const G011_INSN_WINDOW_ALIGNED_LEN: usize = G011_INSN_WINDOW_LEN & !3;

#[derive(Clone, Copy)]
pub(crate) struct WfiPcObservation {
    pub(crate) word_at: Option<u32>,
    pub(crate) word_before: Option<u32>,
    pub(crate) window_has_wfi: bool,
}

impl WfiPcObservation {
    pub(crate) const fn unavailable() -> Self {
        Self {
            word_at: None,
            word_before: None,
            window_has_wfi: false,
        }
    }
    pub(crate) fn is_wfiish(self) -> bool {
        word_is_wait_instruction(self.word_at) || word_is_wait_instruction(self.word_before)
    }
}

pub(crate) struct WfiWakeSummary<'a> {
    pub(crate) stop_reason: &'a str,
    pub(crate) stop_reason_code: Option<u32>,
    pub(crate) exits: u64,
    pub(crate) vtimer_exits: u64,
    pub(crate) final_pc: u64,
    pub(crate) last_prerun_pc: Option<u64>,
    pub(crate) final_pc_observation: WfiPcObservation,
    pub(crate) last_prerun_pc_observation: WfiPcObservation,
    pub(crate) last_nonzero_irq_drain_pc_observation: Option<WfiPcObservation>,
}

impl WfiWakeSummary<'_> {
    pub(crate) fn print(&self, drain_stats: &RunLoopDrainStats) {
        let last_nonzero_location = drain_stats.last_nonzero_location.unwrap_or("<none>");
        let last_nonzero_irq_drain_pc_wfiish = self
            .last_nonzero_irq_drain_pc_observation
            .map(WfiPcObservation::is_wfiish);
        println!("G011 WFI wake-source summary:");
        println!(
            "  stop={} reason_code={} exits={} watchdog_canceled={}",
            self.stop_reason,
            format_optional_u32_hex(self.stop_reason_code),
            self.exits,
            self.stop_reason_code == Some(EXIT_CANCELED)
        );
        println!(
            "  final_pc={:#x} final_pc_wfiish={} final_window_has_wfi={} final_word_at={} final_word_before={}",
            self.final_pc,
            self.final_pc_observation.is_wfiish(),
            self.final_pc_observation.window_has_wfi,
            format_optional_instruction_word(self.final_pc_observation.word_at),
            format_optional_instruction_word(self.final_pc_observation.word_before)
        );
        println!(
            "  last_prerun_pc={} last_prerun_pc_wfiish={} last_prerun_window_has_wfi={} last_prerun_word_at={} last_prerun_word_before={}",
            format_optional_u64_hex(self.last_prerun_pc),
            self.last_prerun_pc_observation.is_wfiish(),
            self.last_prerun_pc_observation.window_has_wfi,
            format_optional_instruction_word(self.last_prerun_pc_observation.word_at),
            format_optional_instruction_word(self.last_prerun_pc_observation.word_before)
        );
        println!(
            "  vtimer_exits={} msix_drained={} spi_drained={} device_event_quiescent_at_stop={}",
            self.vtimer_exits,
            drain_stats.msix.drained,
            drain_stats.spi.drained,
            format_optional_bool(drain_stats.last_drain_was_empty())
        );
        println!(
            "  last_nonzero_irq_drain=location={} exit={} pc={} pc_wfiish={}",
            last_nonzero_location,
            format_optional_u64_dec(drain_stats.last_nonzero_exit),
            format_optional_u64_hex(drain_stats.last_nonzero_pc),
            format_optional_bool(last_nonzero_irq_drain_pc_wfiish)
        );
    }
}

pub(crate) fn word_is_wait_instruction(word: Option<u32>) -> bool {
    matches!(word, Some(ARM64_WFI | ARM64_WFE))
}

pub(crate) fn read_translated_instruction_word(
    mem: &dyn GuestMemoryMut,
    ipa: Option<u64>,
) -> Option<u32> {
    let mut word_bytes = [0u8; 4];
    mem.read_into(ipa?, &mut word_bytes)
        .then(|| u32::from_le_bytes(word_bytes))
}

pub(crate) fn translated_word_before(
    mem: &dyn GuestMemoryMut,
    center_ipa: Option<u64>,
) -> Option<u32> {
    read_translated_instruction_word(mem, center_ipa?.checked_sub(4))
}

pub(crate) fn translated_window_has_wfi(mem: &dyn GuestMemoryMut, center_ipa: Option<u64>) -> bool {
    let Some(center_ipa) = center_ipa else {
        return false;
    };
    let Some(base_ipa) = center_ipa.checked_sub(G011_INSN_WINDOW_BEFORE) else {
        return false;
    };
    let mut bytes = [0u8; G011_INSN_WINDOW_ALIGNED_LEN];
    if !mem.read_into(base_ipa, &mut bytes) {
        return false;
    }
    bytes.chunks_exact(4).any(|chunk| {
        let Ok(word_bytes) = <[u8; 4]>::try_from(chunk) else {
            return false;
        };
        u32::from_le_bytes(word_bytes) == ARM64_WFI
    })
}

pub(crate) fn wfi_pc_observation(
    mem: &dyn GuestMemoryMut,
    center_ipa: Option<u64>,
) -> WfiPcObservation {
    if center_ipa.is_none() {
        return WfiPcObservation::unavailable();
    }
    WfiPcObservation {
        word_at: read_translated_instruction_word(mem, center_ipa),
        word_before: translated_word_before(mem, center_ipa),
        window_has_wfi: translated_window_has_wfi(mem, center_ipa),
    }
}

pub(crate) fn format_optional_bool(value: Option<bool>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| value.to_string())
}

pub(crate) fn format_optional_u32_hex(value: Option<u32>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn format_optional_instruction_word(value: Option<u32>) -> String {
    value.map_or_else(
        || "<unreadable>".to_string(),
        |value| format!("{value:#010x}"),
    )
}

pub(crate) fn format_optional_u64_dec(value: Option<u64>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| value.to_string())
}

pub(crate) fn format_optional_u64_hex(value: Option<u64>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| format!("{value:#x}"))
}

#[cfg(test)]
mod wfi_summary_tests {
    use super::*;

    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl GuestMemoryMut for TestMem {
        fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
            let Some(off) = gpa
                .checked_sub(self.base)
                .and_then(|off| usize::try_from(off).ok())
            else {
                return false;
            };
            if off + data.len() > self.bytes.len() {
                return false;
            }
            self.bytes[off..off + data.len()].copy_from_slice(data);
            true
        }

        fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
            let off = usize::try_from(gpa.checked_sub(self.base)?).ok()?;
            if off + len > self.bytes.len() {
                return None;
            }
            Some(self.bytes[off..off + len].to_vec())
        }
    }

    #[test]
    fn finds_wfi_near_translated_final_pc() {
        let center_ipa = 0x1020;
        let mut mem = TestMem {
            base: 0x1000,
            bytes: vec![0; 0x80],
        };
        assert!(mem.write_bytes(center_ipa - 4, &ARM64_WFI.to_le_bytes()));

        let observation = wfi_pc_observation(&mem, Some(center_ipa));

        assert!(observation.window_has_wfi);
        assert!(observation.is_wfiish());
        assert_eq!(observation.word_before, Some(ARM64_WFI));
    }

    #[test]
    fn reports_no_wfi_when_translation_is_missing() {
        let mem = TestMem {
            base: 0x1000,
            bytes: vec![0; 0x20],
        };

        let observation = wfi_pc_observation(&mem, Some(0x9000));

        assert!(!observation.window_has_wfi);
        assert!(!observation.is_wfiish());
        assert_eq!(observation.word_at, None);
    }
}

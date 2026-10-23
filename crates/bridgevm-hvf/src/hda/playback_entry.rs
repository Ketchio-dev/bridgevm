//! One BDL entry's progress: each DMA chunk goes to the host sink and then to
//! the stale-replay diagnostics; a finished entry raises IOC when flagged and
//! advances; the cursor wraps at CBL, and the position buffer follows LPIB.

use super::*;

impl HdaController {
    /// Hand one DMA chunk (`pcm_scratch`, at the current entry cursor) to the
    /// host sink unchanged, then to the stale-replay diagnostics.
    pub(crate) fn emit_pcm_chunk(&mut self, length: u32, (rate, channels, bits): (u32, u8, u8)) {
        if let Some(output) = self.pcm_sink.as_mut() {
            output.write_pcm(&self.pcm_scratch, rate, channels, bits);
        }
        let (index, offset) = (self.stream.bdl_index, self.stream.bdl_offset);
        self.diagnostics
            .repeats
            .note_chunk(index, offset, length, &self.pcm_scratch);
    }

    pub(crate) fn complete_bdl_entry(&mut self, flags: u32) {
        if flags & BDL_IOC != 0 {
            self.stream.sts |= SDSTS_BCIS;
            self.diagnostics.note_ioc();
            if hda_trace_enabled() {
                println!(
                    "hda: BDL IOC index={} lpib={}",
                    self.stream.bdl_index, self.stream.lpib
                );
            }
        }
        self.stream.bdl_offset = 0;
        self.stream.bdl_index = if self.stream.bdl_index >= self.stream.lvi {
            0
        } else {
            self.stream.bdl_index + 1
        };
    }

    pub(crate) fn wrap_cyclic_buffer(&mut self) {
        self.stream.lpib = 0;
        self.stream.bdl_index = 0;
        self.stream.bdl_offset = 0;
    }

    pub(crate) fn write_position_buffer(&self, mem: &mut dyn GuestMemoryMut) {
        if self.dp_base & 1 == 0 {
            return;
        }
        let base = self.dp_base & !0x7f;
        let _ = mem.write_bytes(base, &self.stream.lpib.to_le_bytes());
    }
}

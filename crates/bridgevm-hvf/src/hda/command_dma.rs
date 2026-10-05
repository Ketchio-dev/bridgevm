//! Command and response DMA with checked guest ring addresses.

use super::*;

impl HdaController {
    pub(crate) fn process_corb(&mut self, mem: &mut dyn GuestMemoryMut) {
        if self.gctl & GCTL_CRST == 0
            || self.corb_ctl & CORBCTL_RUN == 0
            || self.rirb_ctl & RIRBCTL_DMA == 0
        {
            return;
        }
        let mask = self.corb_pointer_mask();
        let mut guard = 0usize;
        let mut produced = false;
        while self.corb_rp != self.corb_wp && guard < usize::from(mask) + 1 {
            let next = self.corb_rp.wrapping_add(1) & mask;
            let mut raw = [0u8; 4];
            let address = self.corb_base.checked_add(u64::from(next) * 4);
            if !address.is_some_and(|address| mem.read_into(address, &mut raw)) {
                self.corb_sts |= CORBSTS_CMEI;
                break;
            }
            let verb = u32::from_le_bytes(raw);
            let response = self.codec_verb(verb);
            self.corb_rp = next;
            if !self.push_rirb(mem, response, (verb >> 28) as u8) {
                break;
            }
            produced = true;
            guard += 1;
        }
        // Raise the RIRB interrupt once the CORB ring drains, even if fewer
        // than RINTCNT responses accumulated. Windows' hdaudio.sys
        // submits small verb batches (e.g. a lone GET_PARAMETER during codec
        // enumeration) and waits for the RIRB interrupt; without this it would
        // never be notified for a sub-threshold batch and times out — which is
        // exactly why it read the AFG basics then stalled before descending to
        // the DAC/pin widgets.
        if produced && self.corb_rp == self.corb_wp && self.rirb_ctl & RIRBCTL_RINTCTL != 0 {
            self.rirb_sts |= RIRBSTS_RINTFL;
            self.responses_since_irq = 0;
        }
    }

    pub(crate) fn push_rirb(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        response: u32,
        codec: u8,
    ) -> bool {
        let next = self.rirb_wp.wrapping_add(1) & self.rirb_pointer_mask();
        let mut entry = [0u8; 8];
        entry[..4].copy_from_slice(&response.to_le_bytes());
        entry[4..].copy_from_slice(&u32::from(codec & 0x0f).to_le_bytes());
        let address = self.rirb_base.checked_add(u64::from(next) * 8);
        if !address.is_some_and(|address| mem.write_bytes(address, &entry)) {
            self.rirb_sts |= RIRBSTS_OIS;
            return false;
        }
        self.rirb_wp = next;
        self.responses_since_irq = self.responses_since_irq.wrapping_add(1);
        let threshold = if self.rintcnt == 0 { 256 } else { self.rintcnt };
        if self.responses_since_irq >= threshold {
            self.rirb_sts |= RIRBSTS_RINTFL;
            self.responses_since_irq = 0;
        }
        if hda_trace_enabled() {
            println!("hda: verb response codec={codec} response={response:#010x} rirb_wp={next}");
        }
        true
    }
}

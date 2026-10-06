//! Playback DMA and guest descriptor failures.

use super::*;

impl HdaController {
    pub(crate) fn consume_stream(&mut self, mem: &mut dyn GuestMemoryMut, mut budget: usize) {
        if budget == 0 || self.stream.cbl == 0 || self.stream.bdl == 0 {
            return;
        }
        let Some((rate, channels, bits)) = stream_pcm_format(self.stream.fmt) else {
            self.stream.sts |= SDSTS_DESE;
            return;
        };
        while budget > 0 && self.stream.ctl & SDCTL_RUN != 0 {
            let descriptor_gpa = self.stream.bdl + u64::from(self.stream.bdl_index) * 16;
            let mut raw = [0u8; 16];
            if !mem.read_into(descriptor_gpa, &mut raw) {
                self.stream.sts |= SDSTS_DESE;
                self.stream.ctl &= !SDCTL_RUN;
                break;
            }
            let address = u64::from_le_bytes(raw[..8].try_into().unwrap());
            let length = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let flags = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            if length == 0 {
                self.stream.sts |= SDSTS_DESE;
                self.stream.ctl &= !SDCTL_RUN;
                break;
            }
            if self.stream.bdl_offset >= length {
                self.complete_bdl_entry(flags);
                continue;
            }
            let remaining_entry = (length - self.stream.bdl_offset) as usize;
            let remaining_cbl = (self.stream.cbl - self.stream.lpib.min(self.stream.cbl)) as usize;
            let chunk_len = budget.min(remaining_entry).min(remaining_cbl);
            if chunk_len == 0 {
                self.wrap_cyclic_buffer();
                self.write_position_buffer(mem);
                continue;
            }
            self.pcm_scratch.resize(chunk_len, 0);
            let dma_address = address.checked_add(u64::from(self.stream.bdl_offset));
            if !dma_address.is_some_and(|address| mem.read_into(address, &mut self.pcm_scratch)) {
                self.stream.sts |= SDSTS_DESE;
                self.stream.ctl &= !SDCTL_RUN;
                break;
            }
            if let Some(output) = self.pcm_sink.as_mut() {
                output.write_pcm(&self.pcm_scratch, rate, channels, bits);
            }
            self.stream.bdl_offset += chunk_len as u32;
            self.stream.lpib += chunk_len as u32;
            budget -= chunk_len;
            self.write_position_buffer(mem);
            if self.stream.bdl_offset == length {
                self.complete_bdl_entry(flags);
            }
            if self.stream.lpib >= self.stream.cbl {
                self.wrap_cyclic_buffer();
                self.write_position_buffer(mem);
            }
        }
    }
}

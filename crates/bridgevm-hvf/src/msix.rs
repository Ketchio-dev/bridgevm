//! MSI-X table / Pending Bit Array model.
//!
//! PCI config space only advertises where an endpoint's MSI-X table and PBA
//! live. The table itself is a BAR-backed device register block, so the endpoint
//! model owns this state and asks the live HVF layer to send any resulting
//! messages.

/// One deliverable MSI-X message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsixMessage {
    pub vector: u16,
    pub address: u64,
    pub data: u32,
}

/// BAR-backed MSI-X table and Pending Bit Array.
#[derive(Debug, Clone)]
pub struct MsixTable {
    entries: Vec<[u8; Self::ENTRY_BYTES as usize]>,
    pending_bits: Vec<u64>,
}

impl MsixTable {
    pub const ENTRY_BYTES: u64 = 16;
    const VECTOR_CONTROL_MASKED: u32 = 1;

    pub fn new(vector_count: u16) -> Self {
        assert!(
            vector_count > 0,
            "MSI-X table must expose at least one vector"
        );
        let mut entries = vec![[0u8; Self::ENTRY_BYTES as usize]; vector_count as usize];
        for entry in &mut entries {
            entry[12..16].copy_from_slice(&Self::VECTOR_CONTROL_MASKED.to_le_bytes());
        }
        let pending_words = usize::from(vector_count).div_ceil(64);
        Self {
            entries,
            pending_bits: vec![0u64; pending_words],
        }
    }

    pub fn vector_count(&self) -> u16 {
        self.entries.len() as u16
    }

    pub fn table_byte_len(&self) -> u64 {
        self.entries.len() as u64 * Self::ENTRY_BYTES
    }

    pub fn pba_byte_len(&self) -> u64 {
        self.pending_bits.len() as u64 * 8
    }

    pub fn table_read(&self, offset: u64, size: u8) -> u64 {
        let mut value = 0u64;
        for byte in 0..usize::from(size.min(8)) {
            let Some(b) = self.table_byte(offset + byte as u64) else {
                continue;
            };
            value |= u64::from(b) << (byte * 8);
        }
        mask_to_size(value, size)
    }

    pub fn table_write(&mut self, offset: u64, size: u8, value: u64) {
        for byte in 0..usize::from(size.min(8)) {
            let off = offset + byte as u64;
            if off >= self.table_byte_len() {
                continue;
            }
            let entry_idx = (off / Self::ENTRY_BYTES) as usize;
            let entry_off = (off % Self::ENTRY_BYTES) as usize;
            self.entries[entry_idx][entry_off] = ((value >> (byte * 8)) & 0xff) as u8;
            self.mask_reserved_vector_control_bits(entry_idx);
        }
    }

    pub fn pba_read(&self, offset: u64, size: u8) -> u64 {
        let mut value = 0u64;
        for byte in 0..usize::from(size.min(8)) {
            let absolute = offset + byte as u64;
            let word_idx = (absolute / 8) as usize;
            if word_idx >= self.pending_bits.len() {
                continue;
            }
            let word_byte = (absolute % 8) as usize;
            value |= ((self.pending_bits[word_idx] >> (word_byte * 8)) & 0xff) << (byte * 8);
        }
        mask_to_size(value, size)
    }

    /// PBA is read-only from the guest's perspective.
    pub fn pba_write(&mut self, _offset: u64, _size: u8, _value: u64) {}

    /// Raise one vector. If MSI-X is disabled, nothing is recorded; while MSI-X
    /// is enabled, function/vector masks defer delivery by setting the PBA bit.
    pub fn raise(
        &mut self,
        vector: u16,
        function_enabled: bool,
        function_masked: bool,
    ) -> Option<MsixMessage> {
        if !function_enabled || usize::from(vector) >= self.entries.len() {
            return None;
        }
        if function_masked || self.vector_masked(vector) {
            self.set_pending(vector);
            return None;
        }
        let Some(message) = self.message(vector) else {
            self.set_pending(vector);
            return None;
        };
        Some(message)
    }

    /// Deliver any pending vectors that became unmasked after a table/config
    /// write. Undeliverable vectors remain pending.
    pub fn drain_pending(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_into(function_enabled, function_masked, &mut messages);
        messages
    }

    /// Deliver pending vectors into caller-owned storage.
    pub fn drain_pending_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if !function_enabled || function_masked {
            return;
        }
        let vector_count = usize::from(self.vector_count());
        for word_idx in 0..self.pending_bits.len() {
            let mut pending_word = self.pending_bits[word_idx];
            while pending_word != 0 {
                let bit = pending_word.trailing_zeros() as usize;
                let vector_idx = word_idx * 64 + bit;
                if vector_idx >= vector_count {
                    break;
                }
                let vector = vector_idx as u16;
                if !self.vector_masked(vector) {
                    if let Some(message) = self.message(vector) {
                        self.clear_pending(vector);
                        out.push(message);
                    }
                }
                pending_word &= !(1u64 << bit);
            }
        }
    }

    pub fn pending(&self, vector: u16) -> bool {
        let word_idx = usize::from(vector) / 64;
        let bit = usize::from(vector) % 64;
        self.pending_bits
            .get(word_idx)
            .is_some_and(|word| (word & (1u64 << bit)) != 0)
    }

    fn table_byte(&self, offset: u64) -> Option<u8> {
        if offset >= self.table_byte_len() {
            return None;
        }
        let entry_idx = (offset / Self::ENTRY_BYTES) as usize;
        let entry_off = (offset % Self::ENTRY_BYTES) as usize;
        Some(self.entries[entry_idx][entry_off])
    }

    fn message(&self, vector: u16) -> Option<MsixMessage> {
        let entry = self.entries.get(usize::from(vector))?;
        let address = u64::from(u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]))
            | (u64::from(u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]])) << 32);
        let data = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
        (address != 0).then_some(MsixMessage {
            vector,
            address,
            data,
        })
    }

    fn vector_masked(&self, vector: u16) -> bool {
        let Some(entry) = self.entries.get(usize::from(vector)) else {
            return true;
        };
        let control = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);
        control & Self::VECTOR_CONTROL_MASKED != 0
    }

    fn set_pending(&mut self, vector: u16) {
        let word_idx = usize::from(vector) / 64;
        let bit = usize::from(vector) % 64;
        if let Some(word) = self.pending_bits.get_mut(word_idx) {
            *word |= 1u64 << bit;
        }
    }

    fn clear_pending(&mut self, vector: u16) {
        let word_idx = usize::from(vector) / 64;
        let bit = usize::from(vector) % 64;
        if let Some(word) = self.pending_bits.get_mut(word_idx) {
            *word &= !(1u64 << bit);
        }
    }

    fn mask_reserved_vector_control_bits(&mut self, entry_idx: usize) {
        let entry = &mut self.entries[entry_idx];
        let control = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]])
            & Self::VECTOR_CONTROL_MASKED;
        entry[12..16].copy_from_slice(&control.to_le_bytes());
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u16(self.vector_count());
        out.write_u16(0);
        for entry in &self.entries {
            out.write_blob(entry);
        }
        out.write_u32(self.pending_bits.len() as u32);
        for word in &self.pending_bits {
            out.write_u64(*word);
        }
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(input.read_u32(), 1, "unsupported MSI-X snapshot version");
        let vectors = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid MSI-X snapshot");
        assert_eq!(
            vectors,
            self.vector_count(),
            "MSI-X vector-count mismatch on restore"
        );

        for entry in &mut self.entries {
            let bytes = input.read_blob();
            assert_eq!(bytes.len(), Self::ENTRY_BYTES as usize);
            entry.copy_from_slice(&bytes);
        }

        let pending_words = input.read_u32() as usize;
        assert_eq!(
            pending_words,
            self.pending_bits.len(),
            "MSI-X PBA size mismatch on restore"
        );
        for word in &mut self.pending_bits {
            *word = input.read_u64();
        }
        input.finish();
    }
}

fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_entries_round_trip_and_start_masked() {
        let mut table = MsixTable::new(2);
        assert_eq!(table.table_read(12, 4), 1);

        table.table_write(0, 8, 0x0000_0001_0808_0000);
        table.table_write(8, 4, 35);
        assert_eq!(table.table_read(0, 8), 0x0000_0001_0808_0000);
        assert_eq!(table.table_read(8, 4), 35);
    }

    #[test]
    fn masked_vector_sets_pba_and_unmask_drains_message() {
        let mut table = MsixTable::new(2);
        table.table_write(0, 8, 0x0808_0000);
        table.table_write(8, 4, 35);

        assert_eq!(table.raise(0, true, false), None);
        assert_eq!(table.pba_read(0, 8), 1);

        table.table_write(12, 4, 0);
        assert_eq!(
            table.drain_pending(true, false),
            vec![MsixMessage {
                vector: 0,
                address: 0x0808_0000,
                data: 35,
            }]
        );
        assert_eq!(table.pba_read(0, 8), 0);
    }

    #[test]
    fn drain_pending_into_appends_to_caller_storage() {
        let mut table = MsixTable::new(2);
        table.table_write(0, 8, 0x0808_0000);
        table.table_write(8, 4, 35);
        table.table_write(16, 8, 0x0808_1000);
        table.table_write(24, 4, 36);

        assert_eq!(table.raise(0, true, false), None);
        assert_eq!(table.raise(1, true, false), None);
        table.table_write(12, 4, 0);
        table.table_write(28, 4, 0);

        let sentinel = MsixMessage {
            vector: 99,
            address: 0xfeed,
            data: 1,
        };
        let mut out = Vec::with_capacity(4);
        out.push(sentinel);
        let ptr = out.as_ptr();
        let capacity = out.capacity();

        table.drain_pending_into(true, false, &mut out);

        assert_eq!(out.capacity(), capacity);
        assert_eq!(out.as_ptr(), ptr);
        assert_eq!(
            out,
            vec![
                sentinel,
                MsixMessage {
                    vector: 0,
                    address: 0x0808_0000,
                    data: 35,
                },
                MsixMessage {
                    vector: 1,
                    address: 0x0808_1000,
                    data: 36,
                },
            ]
        );
    }

    #[test]
    fn drain_pending_into_walks_sparse_pending_bits_and_preserves_undeliverable_vectors() {
        let mut table = MsixTable::new(130);
        table.table_write(0, 8, 0x0808_0000);
        table.table_write(8, 4, 35);
        table.table_write(129 * MsixTable::ENTRY_BYTES, 8, 0x0808_2000);
        table.table_write(129 * MsixTable::ENTRY_BYTES + 8, 4, 36);

        assert_eq!(table.raise(0, true, false), None);
        assert_eq!(table.raise(64, true, false), None);
        assert_eq!(table.raise(129, true, false), None);
        table.table_write(12, 4, 0);
        table.table_write(129 * MsixTable::ENTRY_BYTES + 12, 4, 0);

        let mut out = Vec::new();
        table.drain_pending_into(true, false, &mut out);

        assert_eq!(
            out,
            vec![
                MsixMessage {
                    vector: 0,
                    address: 0x0808_0000,
                    data: 35,
                },
                MsixMessage {
                    vector: 129,
                    address: 0x0808_2000,
                    data: 36,
                },
            ]
        );
        assert_eq!(table.pba_read(0, 8), 0);
        assert!(table.pending(64));
        assert_eq!(table.pba_read(8, 8), 1);
        assert_eq!(table.pba_read(16, 8), 0);
    }

    #[test]
    fn disabled_function_drops_without_setting_pba() {
        let mut table = MsixTable::new(1);
        table.table_write(0, 8, 0x0808_0000);
        table.table_write(8, 4, 35);

        assert_eq!(table.raise(0, false, false), None);
        assert_eq!(table.pba_read(0, 8), 0);
    }
}

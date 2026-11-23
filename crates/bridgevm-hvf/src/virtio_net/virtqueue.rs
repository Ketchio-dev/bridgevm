//! Scatter-writing a received frame across a descriptor chain.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn scatter_write_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        slices: &[&[u8]],
    ) -> bool {
        let total_len = slices
            .iter()
            .try_fold(0usize, |sum, slice| sum.checked_add(slice.len()));
        let Some(total_len) = total_len else {
            return false;
        };
        if total_len == 0 {
            return true;
        }
        if descs.iter().any(|desc| desc.flags & DESC_F_WRITE == 0) {
            return false;
        }

        let mut slice_index = 0usize;
        let mut slice_offset = 0usize;
        let mut written = 0usize;

        for desc in descs {
            let mut desc_offset = 0usize;
            let desc_len = desc.len as usize;
            while desc_offset < desc_len && written < total_len {
                while slice_index < slices.len() && slice_offset == slices[slice_index].len() {
                    slice_index += 1;
                    slice_offset = 0;
                }
                if slice_index == slices.len() {
                    return written == total_len;
                }

                let slice = slices[slice_index];
                let copy_len = (desc_len - desc_offset).min(slice.len() - slice_offset);
                let Some(gpa) = desc.addr.checked_add(desc_offset as u64) else {
                    return false;
                };
                if !mem.write_bytes(gpa, &slice[slice_offset..slice_offset + copy_len]) {
                    return false;
                }
                desc_offset += copy_len;
                slice_offset += copy_len;
                written += copy_len;
            }
        }
        written == total_len
    }
}

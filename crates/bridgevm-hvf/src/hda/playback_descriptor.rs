//! Resolve a saved descriptor index against the guest-programmed BDL base.

use super::*;

impl HdaController {
    pub(crate) fn read_playback_descriptor(&self, mem: &dyn GuestMemoryMut) -> Option<[u8; 16]> {
        let offset = u64::from(self.stream.bdl_index) * 16;
        let address = self.stream.bdl.checked_add(offset)?;
        let mut raw = [0u8; 16];
        mem.read_into(address, &mut raw).then_some(raw)
    }
}

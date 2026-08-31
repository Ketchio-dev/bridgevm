use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

pub(super) const PAGE_ALIGNMENT: usize = 0x1_0000;

pub(super) struct AlignedMemory {
    pub(super) pointer: NonNull<u8>,
    pub(super) layout: Layout,
}

impl AlignedMemory {
    pub(super) fn new(size: usize) -> Result<Self, String> {
        let layout = Layout::from_size_align(size, PAGE_ALIGNMENT)
            .map_err(|error| format!("guest allocation layout: {error}"))?;
        let pointer = NonNull::new(unsafe { alloc_zeroed(layout) })
            .ok_or_else(|| "guest allocation failed".to_string())?;
        Ok(Self { pointer, layout })
    }

    pub(super) fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.layout.size()) }
    }
}

impl Drop for AlignedMemory {
    fn drop(&mut self) {
        unsafe { dealloc(self.pointer.as_ptr(), self.layout) }
    }
}

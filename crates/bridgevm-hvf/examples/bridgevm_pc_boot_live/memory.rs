use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine::bridgevm_pc as board;
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

const ALIGNMENT: usize = 0x1_0000;

pub(super) struct AlignedMemory {
    pub(super) pointer: NonNull<u8>,
    pub(super) layout: Layout,
}

impl AlignedMemory {
    pub(super) fn new(size: usize) -> Result<Self, String> {
        let layout = Layout::from_size_align(size, ALIGNMENT)
            .map_err(|error| format!("guest allocation layout: {error}"))?;
        let pointer = NonNull::new(unsafe { alloc_zeroed(layout) })
            .ok_or_else(|| "guest allocation failed".to_string())?;
        Ok(Self { pointer, layout })
    }

    pub(super) fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.layout.size()) }
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

pub(super) struct GuestRam<'a> {
    memory: &'a mut AlignedMemory,
}

impl<'a> GuestRam<'a> {
    pub(super) fn new(memory: &'a mut AlignedMemory) -> Self {
        Self { memory }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        self.memory.bytes()
    }

    fn range(&self, gpa: u64, len: usize) -> Option<std::ops::Range<usize>> {
        let start = usize::try_from(gpa.checked_sub(board::RAM_BASE)?).ok()?;
        let end = start.checked_add(len)?;
        (end <= self.memory.layout.size()).then_some(start..end)
    }
}

impl GuestMemoryMut for GuestRam<'_> {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(range) = self.range(gpa, data.len()) else {
            return false;
        };
        self.memory.bytes_mut()[range].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        Some(self.bytes()[self.range(gpa, len)?].to_vec())
    }

    fn read_into(&self, gpa: u64, destination: &mut [u8]) -> bool {
        let Some(range) = self.range(gpa, destination.len()) else {
            return false;
        };
        destination.copy_from_slice(&self.bytes()[range]);
        true
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let range = self.range(gpa, len)?;
        Some(self.memory.pointer.as_ptr().wrapping_add(range.start))
    }
}

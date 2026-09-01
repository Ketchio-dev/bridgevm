use super::aligned_memory::AlignedMemory;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;

const TEST_MEDIA_BYTES: usize = 1024 * 1024;
const LBA0_MARKER: &[u8; 8] = b"BRIDGEVM";

pub(super) fn load_test_media(platform: &mut BridgeVmPcPlatform) {
    let mut media = vec![0u8; TEST_MEDIA_BYTES];
    media[..LBA0_MARKER.len()].copy_from_slice(LBA0_MARKER);
    platform.load_nvme_disk_image(media);
}

pub(super) struct GuestRam<'a> {
    memory: &'a mut AlignedMemory,
}

impl<'a> GuestRam<'a> {
    pub(super) fn new(memory: &'a mut AlignedMemory) -> Self {
        Self { memory }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.memory.pointer.as_ptr(), self.memory.layout.size())
        }
    }

    fn range(&self, gpa: u64, len: usize) -> Option<std::ops::Range<usize>> {
        let start = gpa
            .checked_sub(board::RAM_BASE)
            .and_then(|offset| usize::try_from(offset).ok())?;
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
        let range = self.range(gpa, len)?;
        Some(self.bytes()[range].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(range) = self.range(gpa, dst.len()) else {
            return false;
        };
        dst.copy_from_slice(&self.bytes()[range]);
        true
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let range = self.range(gpa, len)?;
        Some(self.memory.pointer.as_ptr().wrapping_add(range.start))
    }
}

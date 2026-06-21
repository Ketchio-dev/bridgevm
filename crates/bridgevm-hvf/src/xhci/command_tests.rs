use super::*;
use crate::fwcfg::GuestMemoryMut;

const DOORBELL_BASE: u64 = 0x2000;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const ENABLE_SLOT_ID: u32 = 1;
const CMD_RING: u64 = 0x1000;
const ERST: u64 = 0x2000;
const EVENT_RING: u64 = 0x3000;
const DCBAA: u64 = 0x4000;

#[derive(Debug)]
struct TestRam {
    bytes: Vec<u8>,
}

impl TestRam {
    fn new(len: usize) -> Self {
        Self {
            bytes: vec![0; len],
        }
    }

    fn write_u32(&mut self, gpa: u64, value: u32) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }

    fn write_u64(&mut self, gpa: u64, value: u64) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }

    fn read_u32(&self, gpa: u64) -> u32 {
        let bytes = self.read_bytes(gpa, 4).unwrap();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn read_u64(&self, gpa: u64) -> u64 {
        let bytes = self.read_bytes(gpa, 8).unwrap();
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }
}

impl GuestMemoryMut for TestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Ok(start) = usize::try_from(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(gpa).ok()?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }
}

fn setup_enable_slot_rings(xhci: &mut XhciController, mem: &mut TestRam) {
    let enable_slot = (TRB_TYPE_ENABLE_SLOT << 10) | 1;
    mem.write_u32(CMD_RING + 12, enable_slot);

    mem.write_u64(ERST, EVENT_RING);
    mem.write_u32(ERST + 8, 16);

    xhci.mmio_write(0x58, 8, CMD_RING | 1);
    xhci.mmio_write(0x70, 8, DCBAA);
    xhci.mmio_write(0x78, 4, 64);
    xhci.mmio_write(0x1028, 4, 1);
    xhci.mmio_write(0x1030, 8, ERST);
    xhci.mmio_write(0x1038, 8, EVENT_RING | 0x8);
    xhci.mmio_write(0x1020, 4, 0x2);
}

#[test]
fn enable_slot_command_posts_success_completion_event() {
    // Given: firmware-style command/event rings containing one Enable Slot TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_enable_slot_rings(&mut xhci, &mut mem);

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event.
    assert_eq!(mem.read_u64(EVENT_RING), CMD_RING);
    assert_eq!(mem.read_u32(EVENT_RING + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(EVENT_RING + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_COMMAND_COMPLETION_EVENT);
    assert_eq!((control >> 24) & 0xff, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT),
        u64::from(USB_STS_EINT)
    );

    xhci.mmio_write(0x1020, 4, 1);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 0);
    assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT), 0);
}

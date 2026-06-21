use super::*;
use crate::fwcfg::GuestMemoryMut;

const DOORBELL_BASE: u64 = 0x2000;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_DISABLE_SLOT: u32 = 10;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const ENABLE_SLOT_ID: u32 = 1;
const ADDRESS_DEVICE_SLOT_ID: u32 = 7;
const DISABLE_SLOT_ID: u32 = 4;
const TRB_SIZE: u64 = 16;
const CMD_RING: u64 = 0x1000;
const LINK_TARGET: u64 = 0x1110;
const ERST: u64 = 0x2000;
const EVENT_RING: u64 = 0x3000;
const LINK_TOGGLE_CYCLE: u32 = 1 << 1;
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

fn setup_command_rings(xhci: &mut XhciController, mem: &mut TestRam, command_control: u32) {
    mem.write_u32(CMD_RING + 12, command_control);
    setup_event_ring(xhci, mem);
}

fn setup_event_ring(xhci: &mut XhciController, mem: &mut TestRam) {
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

fn command_control(trb_type: u32, slot_id: u32) -> u32 {
    command_control_with_cycle(trb_type, slot_id, true)
}

fn command_control_with_cycle(trb_type: u32, slot_id: u32, cycle: bool) -> u32 {
    let cycle_bit = u32::from(cycle);
    (slot_id << 24) | (trb_type << 10) | cycle_bit
}

fn assert_success_completion(
    mem: &TestRam,
    event_gpa: u64,
    command_gpa: u64,
    expected_slot_id: u32,
) {
    assert_eq!(mem.read_u64(event_gpa), command_gpa);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_COMMAND_COMPLETION_EVENT);
    assert_eq!((control >> 24) & 0xff, expected_slot_id);
    assert_eq!(control & 1, 1);
}

#[test]
fn enable_slot_command_posts_success_completion_event() {
    // Given: firmware-style command/event rings containing one Enable Slot TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT),
        u64::from(USB_STS_EINT)
    );

    xhci.mmio_write(0x1020, 4, 1);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 0);
    assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT), 0);
}

#[test]
fn address_device_command_posts_success_completion_event() {
    // Given: a command/event ring pair containing one Address Device TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ADDRESS_DEVICE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event for that slot.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ADDRESS_DEVICE_SLOT_ID);
}

#[test]
fn disable_slot_command_posts_success_completion_event() {
    // Given: a command/event ring pair containing one Disable Slot TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_DISABLE_SLOT, DISABLE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event for that slot.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, DISABLE_SLOT_ID);
}

#[test]
fn command_doorbell_advances_internal_dequeue_to_address_device() {
    // Given: a command ring with Enable Slot followed by Address Device.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    mem.write_u32(
        CMD_RING + 12,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    mem.write_u32(
        CMD_RING + TRB_SIZE + 12,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    setup_event_ring(&mut xhci, &mut mem);

    // When: the guest rings host-controller doorbell 0 once per command.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: the second completion names the second command TRB.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_success_completion(
        &mem,
        EVENT_RING + TRB_SIZE,
        CMD_RING + TRB_SIZE,
        ENABLE_SLOT_ID,
    );
}

#[test]
fn erdp_update_keeps_next_event_enqueue_slot() {
    // Given: the guest has a command ring with two commands and one event ring segment.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    mem.write_u32(
        CMD_RING + 12,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    mem.write_u32(
        CMD_RING + TRB_SIZE + 12,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    setup_event_ring(&mut xhci, &mut mem);

    // When: firmware consumes the first event and updates ERDP before ringing again.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write(0x1038, 8, (EVENT_RING + TRB_SIZE) | 0x8);
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: ERDP bookkeeping does not rewind the producer enqueue slot.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_success_completion(
        &mem,
        EVENT_RING + TRB_SIZE,
        CMD_RING + TRB_SIZE,
        ENABLE_SLOT_ID,
    );
}

#[test]
fn command_doorbell_follows_link_trb_to_address_device() {
    // Given: Enable Slot is followed by a Link TRB to an Address Device command.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    mem.write_u32(
        CMD_RING + 12,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    mem.write_u64(CMD_RING + TRB_SIZE, LINK_TARGET);
    mem.write_u32(
        CMD_RING + TRB_SIZE + 12,
        command_control(TRB_TYPE_LINK, 0) | LINK_TOGGLE_CYCLE,
    );
    mem.write_u32(
        LINK_TARGET + 12,
        command_control_with_cycle(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID, false),
    );
    setup_event_ring(&mut xhci, &mut mem);

    // When: the guest rings host-controller doorbell 0 once per command.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: the second completion names the linked Address Device command.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_success_completion(&mem, EVENT_RING + TRB_SIZE, LINK_TARGET, ENABLE_SLOT_ID);
}

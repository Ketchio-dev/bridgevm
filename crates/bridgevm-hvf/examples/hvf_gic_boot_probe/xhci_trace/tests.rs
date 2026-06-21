use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const EP0_RING: u64 = 0x3000;

#[derive(Debug)]
struct TestMemory {
    bytes: Vec<u8>,
}

impl TestMemory {
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
}

impl GuestMemoryMut for TestMemory {
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

fn xhci_target(offset: u64) -> PcieMmioTarget {
    PcieMmioTarget {
        bdf: pcie::XHCI_BDF,
        bar_index: 0,
        offset,
    }
}

fn command_control(trb_type: u32, slot: u32) -> u32 {
    (slot << 24) | (trb_type << trb::TYPE_SHIFT) | trb::CYCLE
}

#[test]
fn address_device_command_records_ep0_dequeue_from_input_context() {
    let mut mem = TestMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u64(COMMAND_RING, INPUT_CONTEXT);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_ADDRESS_DEVICE, 1),
    );
    mem.write_u64(
        INPUT_CONTEXT + EP0_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        EP0_RING | trb::CYCLE as u64,
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    assert!(trace.events.iter().any(
        |event| event.contains("address_device slot=1") && event.contains("ep0_dequeue=0x3000")
    ));
}

#[test]
fn split_crcr_write_preserves_cycle_bit_for_command_trace() {
    let mut mem = TestMemory::new(0x2_0000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u32(COMMAND_RING + 12, command_control(trb::TYPE_ENABLE_SLOT, 0));

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 4,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(CRCR_HI)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("dequeue=0x1000 cycle=true")));
    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("type=enable_slot")));
    assert!(!trace
        .events
        .iter()
        .any(|event| event.contains("cycle_mismatch")));
}

#[test]
fn slot_doorbell_records_setup_stage_trb_details() {
    let mut mem = TestMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u64(COMMAND_RING, INPUT_CONTEXT);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_ADDRESS_DEVICE, 1),
    );
    mem.write_u64(
        INPUT_CONTEXT + EP0_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        EP0_RING | trb::CYCLE as u64,
    );
    let get_descriptor = u64::from_le_bytes([0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00]);
    mem.write_u64(EP0_RING, get_descriptor);
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(
        EP0_RING + 12,
        (trb::TYPE_SETUP_STAGE << trb::TYPE_SHIFT) | trb::CYCLE,
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE + DOORBELL_STRIDE)),
        &MmioOp::Write { size: 4, value: 1 },
        &mem,
    );

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("setup_stage")
            && event.contains("bm=0x80")
            && event.contains("req=0x06")
            && event.contains("value=0x0100")
            && event.contains("len=18")));
}

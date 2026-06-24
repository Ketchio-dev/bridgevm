use crate::{fwcfg, xhci};

pub(super) const WINDOWS_ARM_XHCI_HID_BOOT_KEY_USAGE_PAGE: u16 = 0x07;
pub(super) const WINDOWS_ARM_XHCI_HID_BOOT_KEY_USAGE_ID: u16 = 0x2c;

const WINDOWS_ARM_XHCI_HID_PROBE_RAM_BYTES: usize = 0x9000;
const WINDOWS_ARM_XHCI_HID_DOORBELL_BASE: u64 = 0x2000;
const WINDOWS_ARM_XHCI_HID_TRB_SIZE: u64 = 16;
const WINDOWS_ARM_XHCI_HID_CMD_RING: u64 = 0x1000;
const WINDOWS_ARM_XHCI_HID_EVENT_RING: u64 = 0x3000;
const WINDOWS_ARM_XHCI_HID_ERST: u64 = 0x2000;
const WINDOWS_ARM_XHCI_HID_DCBAA: u64 = 0x4000;
const WINDOWS_ARM_XHCI_HID_INPUT_CONTEXT: u64 = 0x5000;
const WINDOWS_ARM_XHCI_HID_DCI3_RING: u64 = 0x6000;
const WINDOWS_ARM_XHCI_HID_DCI3_BUFFER: u64 = 0x6800;
const WINDOWS_ARM_XHCI_HID_DCI3_RELEASE_BUFFER: u64 = 0x6820;
const WINDOWS_ARM_XHCI_HID_OUTPUT_CONTEXT: u64 = 0x7000;
const WINDOWS_ARM_XHCI_HID_ENABLE_SLOT_ID: u32 = 1;
const WINDOWS_ARM_XHCI_HID_DCI3: u32 = 3;
const WINDOWS_ARM_XHCI_HID_TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
const WINDOWS_ARM_XHCI_HID_TRB_TYPE_NORMAL: u32 = 1;
const WINDOWS_ARM_XHCI_HID_TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const WINDOWS_ARM_XHCI_HID_INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const WINDOWS_ARM_XHCI_HID_DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
const WINDOWS_ARM_XHCI_HID_EP_CONTEXT_DWORD1_OFFSET: u64 = 0x04;
const WINDOWS_ARM_XHCI_HID_EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
const WINDOWS_ARM_XHCI_HID_EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
const WINDOWS_ARM_XHCI_HID_DCI3_ADD_CONTEXT_FLAG: u32 = 1 << WINDOWS_ARM_XHCI_HID_DCI3;
const WINDOWS_ARM_XHCI_HID_DCI3_DWORD1: u32 = (3 << 1) | (3 << 3) | (8 << 16);
const WINDOWS_ARM_XHCI_HID_DCI3_DWORD4: u32 = 8;
const WINDOWS_ARM_XHCI_HID_TRB_CYCLE_U32: u32 = 1;
const WINDOWS_ARM_XHCI_HID_TRB_CYCLE_U64: u64 = 1;
const WINDOWS_ARM_XHCI_HID_COMPLETION_CODE_SUCCESS: u32 = 1;

pub(super) struct WindowsArmXhciHidProbeOutcome {
    pub(super) key_report: [u8; 8],
    pub(super) release_report: [u8; 8],
    pub(super) transfer_events: usize,
    pub(super) blockers: Vec<String>,
}

pub(super) fn run_windows_arm_xhci_hid_probe() -> WindowsArmXhciHidProbeOutcome {
    let mut blockers = Vec::new();
    let mut xhci = xhci::XhciController::new();
    let mut mem = WindowsArmXhciHidProbeRam::new(WINDOWS_ARM_XHCI_HID_PROBE_RAM_BYTES);
    setup_windows_arm_xhci_hid_probe(&mut xhci, &mut mem);

    if !xhci.mmio_write_with_mem(WINDOWS_ARM_XHCI_HID_DOORBELL_BASE, 4, 0, &mut mem) {
        blockers.push("xHCI Configure Endpoint command did not complete".to_string());
    }
    if !xhci.queue_boot_keyboard_space() {
        blockers.push("xHCI boot keyboard Space report was not queued".to_string());
    }
    if !xhci.mmio_write_with_mem(
        WINDOWS_ARM_XHCI_HID_DOORBELL_BASE + 4,
        4,
        u64::from(WINDOWS_ARM_XHCI_HID_DCI3),
        &mut mem,
    ) {
        blockers.push("xHCI DCI3 key transfer event did not post".to_string());
    }
    if !xhci.mmio_write_with_mem(
        WINDOWS_ARM_XHCI_HID_DOORBELL_BASE + 4,
        4,
        u64::from(WINDOWS_ARM_XHCI_HID_DCI3),
        &mut mem,
    ) {
        blockers.push("xHCI DCI3 release transfer event did not post".to_string());
    }

    let key_report = mem
        .read_report(WINDOWS_ARM_XHCI_HID_DCI3_BUFFER)
        .unwrap_or_default();
    let release_report = mem
        .read_report(WINDOWS_ARM_XHCI_HID_DCI3_RELEASE_BUFFER)
        .unwrap_or_default();

    WindowsArmXhciHidProbeOutcome {
        key_report,
        release_report,
        transfer_events: count_windows_arm_xhci_hid_transfer_events(&mem),
        blockers,
    }
}

#[derive(Debug)]
struct WindowsArmXhciHidProbeRam {
    bytes: Vec<u8>,
}

impl WindowsArmXhciHidProbeRam {
    fn new(len: usize) -> Self {
        Self {
            bytes: vec![0; len],
        }
    }

    fn write_u32(&mut self, gpa: u64, value: u32) {
        let _ = self.write_bytes_at(gpa, &value.to_le_bytes());
    }

    fn write_u64(&mut self, gpa: u64, value: u64) {
        let _ = self.write_bytes_at(gpa, &value.to_le_bytes());
    }

    fn read_u32(&self, gpa: u64) -> Option<u32> {
        let bytes = self.read_bytes_at(gpa, 4)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    fn read_report(&self, gpa: u64) -> Option<[u8; 8]> {
        self.read_bytes_at(gpa, 8)?.try_into().ok()
    }

    fn write_bytes_at(&mut self, gpa: u64, data: &[u8]) -> bool {
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

    fn read_bytes_at(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(gpa).ok()?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }
}

impl fwcfg::GuestMemoryMut for WindowsArmXhciHidProbeRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        self.write_bytes_at(gpa, data)
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.read_bytes_at(gpa, len)
    }
}

fn setup_windows_arm_xhci_hid_probe(
    xhci: &mut xhci::XhciController,
    mem: &mut WindowsArmXhciHidProbeRam,
) {
    mem.write_u64(
        WINDOWS_ARM_XHCI_HID_CMD_RING,
        WINDOWS_ARM_XHCI_HID_INPUT_CONTEXT,
    );
    mem.write_u32(
        WINDOWS_ARM_XHCI_HID_CMD_RING + 12,
        windows_arm_xhci_hid_command_control(
            WINDOWS_ARM_XHCI_HID_TRB_TYPE_CONFIGURE_ENDPOINT,
            WINDOWS_ARM_XHCI_HID_ENABLE_SLOT_ID,
        ),
    );
    mem.write_u64(WINDOWS_ARM_XHCI_HID_ERST, WINDOWS_ARM_XHCI_HID_EVENT_RING);
    mem.write_u32(WINDOWS_ARM_XHCI_HID_ERST + 8, 16);
    xhci.mmio_write(0x58, 8, WINDOWS_ARM_XHCI_HID_CMD_RING | 1);
    xhci.mmio_write(0x70, 8, WINDOWS_ARM_XHCI_HID_DCBAA);
    xhci.mmio_write(0x78, 4, 64);
    xhci.mmio_write(0x1028, 4, 1);
    xhci.mmio_write(0x1030, 8, WINDOWS_ARM_XHCI_HID_ERST);
    xhci.mmio_write(0x1038, 8, WINDOWS_ARM_XHCI_HID_EVENT_RING | 0x8);
    xhci.mmio_write(0x1020, 4, 0x2);
    mem.write_u64(
        WINDOWS_ARM_XHCI_HID_DCBAA + (u64::from(WINDOWS_ARM_XHCI_HID_ENABLE_SLOT_ID) * 8),
        WINDOWS_ARM_XHCI_HID_OUTPUT_CONTEXT,
    );
    mem.write_u32(
        WINDOWS_ARM_XHCI_HID_INPUT_CONTEXT + WINDOWS_ARM_XHCI_HID_INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        WINDOWS_ARM_XHCI_HID_DCI3_ADD_CONTEXT_FLAG,
    );
    mem.write_u32(
        WINDOWS_ARM_XHCI_HID_INPUT_CONTEXT
            + WINDOWS_ARM_XHCI_HID_DCI3_INPUT_CONTEXT_OFFSET
            + WINDOWS_ARM_XHCI_HID_EP_CONTEXT_DWORD1_OFFSET,
        WINDOWS_ARM_XHCI_HID_DCI3_DWORD1,
    );
    mem.write_u64(
        WINDOWS_ARM_XHCI_HID_INPUT_CONTEXT
            + WINDOWS_ARM_XHCI_HID_DCI3_INPUT_CONTEXT_OFFSET
            + WINDOWS_ARM_XHCI_HID_EP_TR_DEQUEUE_OFFSET,
        WINDOWS_ARM_XHCI_HID_DCI3_RING | WINDOWS_ARM_XHCI_HID_TRB_CYCLE_U64,
    );
    mem.write_u32(
        WINDOWS_ARM_XHCI_HID_INPUT_CONTEXT
            + WINDOWS_ARM_XHCI_HID_DCI3_INPUT_CONTEXT_OFFSET
            + WINDOWS_ARM_XHCI_HID_EP_CONTEXT_DWORD4_OFFSET,
        WINDOWS_ARM_XHCI_HID_DCI3_DWORD4,
    );
    write_windows_arm_xhci_hid_normal_trb(
        mem,
        WINDOWS_ARM_XHCI_HID_DCI3_RING,
        WINDOWS_ARM_XHCI_HID_DCI3_BUFFER,
    );
    write_windows_arm_xhci_hid_normal_trb(
        mem,
        WINDOWS_ARM_XHCI_HID_DCI3_RING + WINDOWS_ARM_XHCI_HID_TRB_SIZE,
        WINDOWS_ARM_XHCI_HID_DCI3_RELEASE_BUFFER,
    );
}

fn windows_arm_xhci_hid_command_control(trb_type: u32, slot_id: u32) -> u32 {
    (slot_id << 24) | (trb_type << 10) | WINDOWS_ARM_XHCI_HID_TRB_CYCLE_U32
}

fn write_windows_arm_xhci_hid_normal_trb(
    mem: &mut WindowsArmXhciHidProbeRam,
    trb_gpa: u64,
    buffer_gpa: u64,
) {
    mem.write_u64(trb_gpa, buffer_gpa);
    mem.write_u32(trb_gpa + 8, 8);
    mem.write_u32(
        trb_gpa + 12,
        (WINDOWS_ARM_XHCI_HID_TRB_TYPE_NORMAL << 10) | WINDOWS_ARM_XHCI_HID_TRB_CYCLE_U32,
    );
}

fn count_windows_arm_xhci_hid_transfer_events(mem: &WindowsArmXhciHidProbeRam) -> usize {
    [
        WINDOWS_ARM_XHCI_HID_EVENT_RING + WINDOWS_ARM_XHCI_HID_TRB_SIZE,
        WINDOWS_ARM_XHCI_HID_EVENT_RING + (WINDOWS_ARM_XHCI_HID_TRB_SIZE * 2),
    ]
    .into_iter()
    .filter(|event_gpa| windows_arm_xhci_hid_transfer_event_posted(mem, *event_gpa))
    .count()
}

fn windows_arm_xhci_hid_transfer_event_posted(
    mem: &WindowsArmXhciHidProbeRam,
    event_gpa: u64,
) -> bool {
    let Some(status) = mem.read_u32(event_gpa + 8) else {
        return false;
    };
    let Some(control) = mem.read_u32(event_gpa + 12) else {
        return false;
    };
    let completion_code = status >> 24;
    let trb_type = (control >> 10) & 0x3f;
    let endpoint_id = (control >> 16) & 0x1f;
    let slot_id = (control >> 24) & 0xff;
    completion_code == WINDOWS_ARM_XHCI_HID_COMPLETION_CODE_SUCCESS
        && trb_type == WINDOWS_ARM_XHCI_HID_TRB_TYPE_TRANSFER_EVENT
        && endpoint_id == WINDOWS_ARM_XHCI_HID_DCI3
        && slot_id == WINDOWS_ARM_XHCI_HID_ENABLE_SLOT_ID
        && control & WINDOWS_ARM_XHCI_HID_TRB_CYCLE_U32 != 0
}

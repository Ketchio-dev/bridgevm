use bridgevm_hvf::fwcfg::GuestMemoryMut;

pub(super) const BYTES: usize = 16;
pub(super) const BYTES_U64: u64 = 16;
pub(super) const CYCLE: u32 = 1;
pub(super) const TYPE_SETUP_STAGE: u32 = 2;
pub(super) const TYPE_LINK: u32 = 6;
pub(super) const TYPE_ENABLE_SLOT: u32 = 9;
pub(super) const TYPE_DISABLE_SLOT: u32 = 10;
pub(super) const TYPE_ADDRESS_DEVICE: u32 = 11;
pub(super) const TYPE_CONFIGURE_ENDPOINT: u32 = 12;
pub(super) const TYPE_EVALUATE_CONTEXT: u32 = 13;
pub(super) const TYPE_STOP_ENDPOINT: u32 = 15;
pub(super) const TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
pub(super) const TYPE_RESET_DEVICE: u32 = 17;

const TYPE_NORMAL: u32 = 1;
const TYPE_DATA_STAGE: u32 = 3;
const TYPE_STATUS_STAGE: u32 = 4;
pub(super) const TYPE_SHIFT: u32 = 10;
const TYPE_MASK: u32 = 0x3f;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Trb {
    pub(super) parameter: u64,
    pub(super) status: u32,
    pub(super) control: u32,
}

impl Trb {
    pub(super) fn read_from(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let raw = mem.read_bytes(gpa, BYTES)?;
        Some(Self {
            parameter: read_u64(&raw, 0)?,
            status: read_u32(&raw, 8)?,
            control: read_u32(&raw, 12)?,
        })
    }

    pub(super) const fn kind(self) -> u32 {
        (self.control >> TYPE_SHIFT) & TYPE_MASK
    }

    pub(super) const fn cycle_bit(self) -> u32 {
        self.control & CYCLE
    }

    pub(super) const fn slot_id(self) -> u32 {
        (self.control >> 24) & 0xff
    }

    pub(super) const fn endpoint_id(self) -> u32 {
        (self.control >> 16) & 0x1f
    }

    pub(super) fn kind_name(self) -> String {
        match self.kind() {
            TYPE_NORMAL => "normal".to_string(),
            TYPE_SETUP_STAGE => "setup_stage".to_string(),
            TYPE_DATA_STAGE => "data_stage".to_string(),
            TYPE_STATUS_STAGE => "status_stage".to_string(),
            TYPE_LINK => "link".to_string(),
            TYPE_ENABLE_SLOT => "enable_slot".to_string(),
            TYPE_DISABLE_SLOT => "disable_slot".to_string(),
            TYPE_ADDRESS_DEVICE => "address_device".to_string(),
            TYPE_CONFIGURE_ENDPOINT => "configure_endpoint".to_string(),
            TYPE_EVALUATE_CONTEXT => "evaluate_context".to_string(),
            TYPE_STOP_ENDPOINT => "stop_endpoint".to_string(),
            TYPE_SET_TR_DEQUEUE_POINTER => "set_tr_dequeue_pointer".to_string(),
            TYPE_RESET_DEVICE => "reset_device".to_string(),
            other => format!("type{other}"),
        }
    }

    pub(super) fn setup_description(self) -> String {
        if self.kind() != TYPE_SETUP_STAGE {
            return String::new();
        }
        let bytes = self.parameter.to_le_bytes();
        let request_type = bytes[0];
        let request = bytes[1];
        let value = u16::from_le_bytes([bytes[2], bytes[3]]);
        let index = u16::from_le_bytes([bytes[4], bytes[5]]);
        let length = u16::from_le_bytes([bytes[6], bytes[7]]);
        format!(
            " setup bm={request_type:#04x} req={request:#04x} value={value:#06x} index={index:#06x} len={length}"
        )
    }
}

pub(super) fn read_guest_u64(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u64> {
    mem.read_bytes(gpa, 8).and_then(|bytes| read_u64(&bytes, 0))
}

pub(super) fn read_guest_u32(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u32> {
    mem.read_bytes(gpa, 4).and_then(|bytes| read_u32(&bytes, 0))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    let array: [u8; 4] = raw.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}

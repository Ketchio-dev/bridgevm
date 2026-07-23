//! Split out of virtio_console.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixMessage;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_CONSOLE_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_CONSOLE_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_CONSOLE_MSIX_VECTOR_COUNT;

impl Default for VirtioConsole {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct VirtioPciConsole {
    pub(crate) console: VirtioConsole,
    pub(crate) msix: MsixTable,
}

impl VirtioPciConsole {
    pub fn new() -> Self {
        Self {
            console: VirtioConsole::new(),
            msix: MsixTable::new(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT),
        }
    }

    pub fn stats(&self) -> VirtioConsoleStats {
        self.console.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.console.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.console.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT);
    }

    pub fn agent_send(&mut self, data: &[u8]) {
        self.console.agent_send(data);
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        self.console.take_inbound()
    }

    pub fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        self.console.drain_inbound_into(out);
    }

    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.console.poll(mem)
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciConsoleOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioConsoleResult {
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    self.console
                        .access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.console
                        .access_common(common_offset, true, size, value, mem)
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.console.config_read(device_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.console.config_write(device_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
                VirtioPciConsoleOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.console.notify_queue(queue, mem);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciConsoleOp::Read { size } => VirtioConsoleResult::ReadValue(mask_to_size(
                    u64::from(self.console.interrupt_status),
                    size,
                )),
                VirtioPciConsoleOp::Write { value, .. } => {
                    self.console.interrupt_status &= !(value as u32);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
            VirtioPciConsoleOp::Write { .. } => VirtioConsoleResult::WriteAck,
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciConsoleOp) -> VirtioConsoleResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
            VirtioPciConsoleOp::Write { .. } => VirtioConsoleResult::WriteAck,
        }
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.clear_pending_queue_for_vector(message.vector);
        }
        self.raise_pending_msix_into(function_enabled, function_masked, out);
    }

    pub(crate) fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let mut pending = self.console.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.console.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.console.queues[queue_index].pending_msix = false;
                self.console.pending_msix_queue_bits &= !(1u8 << queue_index);
                out.push(message);
            }
            pending &= !(1u8 << queue_index);
        }
    }

    pub(crate) fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.console.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.console.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_CONSOLE_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let console = &self.console;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(console.device_features_sel);
        out.write_u32(console.driver_features_sel);
        out.write_u32(console.driver_features[0]);
        out.write_u32(console.driver_features[1]);
        out.write_u16(console.config_msix_vector);
        out.write_u16(0);
        out.write_u32(console.queue_sel);
        out.write_u8(console.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(console.status);
        out.write_u32(console.interrupt_status);
        out.write_u32(console.emerg_wr);

        for queue in &console.queues {
            out.write_u16(queue.size);
            out.write_bool(queue.ready);
            out.write_bool(queue.pending_msix);
            out.write_u64(queue.desc);
            out.write_u64(queue.driver);
            out.write_u64(queue.device);
            out.write_u16(queue.msix_vector);
            out.write_u16(queue.notify_off);
            out.write_u16(queue.last_avail_idx);
            out.write_u16(queue.last_avail_seen);
            out.write_u64(queue.notify_count);
            out.write_u64(queue.used_produced);
            out.write_u64(queue.rx_no_buffers);
        }

        for port in &console.ports {
            out.write_bool(port.ready);
            out.write_bool(port.guest_open);
            out.write_bool(port.host_open);
            out.write_u8(0);
        }

        out.write_u32(console.pending_control.len() as u32);
        for message in &console.pending_control {
            out.write_blob(message.as_slice());
        }

        out.write_blob(&console.host_to_guest.iter().copied().collect::<Vec<_>>());
        out.write_blob(&console.host_inbound);
        out.write_bool(console.agent_connected_confirmed);
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-console snapshot version"
        );

        let console = &mut self.console;
        console.device_features_sel = input.read_u32();
        console.driver_features_sel = input.read_u32();
        console.driver_features = [input.read_u32(), input.read_u32()];
        console.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-console snapshot");
        console.queue_sel = input.read_u32();
        console.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-console snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-console snapshot");
        console.status = input.read_u32();
        console.interrupt_status = input.read_u32();
        console.emerg_wr = input.read_u32();

        for queue in &mut console.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            queue.last_avail_seen = input.read_u16();
            queue.notify_count = input.read_u64();
            queue.used_produced = input.read_u64();
            queue.rx_no_buffers = input.read_u64();
        }

        for port in &mut console.ports {
            port.ready = input.read_bool();
            port.guest_open = input.read_bool();
            port.host_open = input.read_bool();
            assert_eq!(input.read_u8(), 0, "invalid console port snapshot");
        }

        console.pending_control.clear();
        let pending_count = input.read_u32() as usize;
        for _ in 0..pending_count {
            let bytes = input.read_blob();
            assert!(
                bytes.len() <= MAX_CONTROL_MESSAGE_LEN,
                "oversized restored console control message"
            );
            console
                .pending_control
                .push_back(PendingControlMessage::from_slice(&bytes));
        }

        console.host_to_guest = input.read_blob().into();
        console.host_inbound = input.read_blob();
        console.agent_connected_confirmed = input.read_bool();
        console.descriptor_scratch.clear();
        console.read_scratch.clear();

        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}

impl Default for VirtioPciConsole {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn common_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_COMMON_CFG_OFFSET)
}

pub(crate) fn device_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_DEVICE_CFG_OFFSET)
}

pub(crate) fn notify_queue_index(offset: u64) -> Option<u16> {
    let rel = offset.checked_sub(PCI_NOTIFY_CFG_OFFSET)?;
    (rel < PCI_CFG_REGION_SIZE).then_some((rel / 4) as u16)
}

pub(crate) fn queue_bit(index: usize) -> Option<u8> {
    (index < u8::BITS as usize).then(|| 1u8 << index)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; 16];
        if !mem.read_into(gpa, &mut bytes) {
            return None;
        }
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Control {
    pub(crate) id: u32,
    pub(crate) event: u16,
    pub(crate) value: u16,
}

impl Control {
    pub(crate) const fn new(id: u32, event: u16, value: u16) -> Self {
        Self { id, event, value }
    }

    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CONTROL_LEN {
            return None;
        }
        Some(Self {
            id: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            event: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            value: u16::from_le_bytes(bytes[6..8].try_into().ok()?),
        })
    }

    pub(crate) fn bytes(self) -> [u8; CONTROL_LEN] {
        let mut out = [0u8; CONTROL_LEN];
        out[0..4].copy_from_slice(&self.id.to_le_bytes());
        out[4..6].copy_from_slice(&self.event.to_le_bytes());
        out[6..8].copy_from_slice(&self.value.to_le_bytes());
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingControlMessage {
    pub(crate) len: usize,
    pub(crate) bytes: [u8; MAX_CONTROL_MESSAGE_LEN],
}

impl PendingControlMessage {
    pub(crate) fn from_slice(bytes: &[u8]) -> Self {
        assert!(bytes.len() <= MAX_CONTROL_MESSAGE_LEN);
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..bytes.len()].copy_from_slice(bytes);
        Self {
            len: bytes.len(),
            bytes: out,
        }
    }

    pub(crate) fn agent_port_name() -> Self {
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..CONTROL_LEN]
            .copy_from_slice(&Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_NAME, 0).bytes());
        out[CONTROL_LEN..MAX_CONTROL_MESSAGE_LEN].copy_from_slice(AGENT_PORT_NAME);
        Self {
            len: MAX_CONTROL_MESSAGE_LEN,
            bytes: out,
        }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

impl From<Control> for PendingControlMessage {
    fn from(control: Control) -> Self {
        Self::from_slice(&control.bytes())
    }
}

pub(crate) fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

pub(crate) fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

pub(crate) fn offered_features_word(select: u32) -> u32 {
    match select {
        0 => VIRTIO_CONSOLE_F_MULTIPORT,
        1 => VIRTIO_F_VERSION_1,
        _ => 0,
    }
}

pub(crate) fn is_supported_common_access_size(size: u8) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

pub(crate) fn common_access_touches(base: u64, width: u8, offset: u64, size: u8) -> bool {
    let access_end = offset.saturating_add(u64::from(size));
    let field_end = base + u64::from(width);
    offset < field_end && base < access_end
}

pub(crate) fn common_access_touches_queue_field(offset: u64, size: u8) -> bool {
    [
        (COMMON_QUEUE_SIZE, 2),
        (COMMON_QUEUE_MSIX_VECTOR, 2),
        (COMMON_QUEUE_ENABLE, 2),
        (COMMON_QUEUE_DESC, 8),
        (COMMON_QUEUE_DRIVER, 8),
        (COMMON_QUEUE_DEVICE, 8),
    ]
    .iter()
    .any(|(base, width)| common_access_touches(*base, *width, offset, size))
}

pub(crate) fn read_common_register(
    base: u64,
    width: u8,
    value: u64,
    offset: u64,
    size: u8,
) -> Option<u64> {
    if !common_access_touches(base, width, offset, size) {
        return None;
    }
    let mut out = 0u64;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let byte = (value >> (field_byte * 8)) & 0xff;
        out |= byte << (u64::from(access_byte) * 8);
    }
    Some(mask_to_size(out, size))
}

pub(crate) fn write_common_register(
    current: u64,
    base: u64,
    width: u8,
    offset: u64,
    size: u8,
    value: u64,
) -> u64 {
    let mut out = current;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let shift = field_byte * 8;
        let byte = (value >> (u64::from(access_byte) * 8)) & 0xff;
        out = (out & !(0xff << shift)) | (byte << shift);
    }
    let bits = u64::from(width) * 8;
    if bits == 64 {
        out
    } else {
        out & ((1u64 << bits) - 1)
    }
}

pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn read_le_from_bytes(bytes: &[u8], offset: u64, size: u8) -> Option<u64> {
    let offset = usize::try_from(offset).ok()?;
    let size = usize::from(size);
    if offset.checked_add(size)? > bytes.len() || size > 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&bytes[offset..offset + size]);
    Some(u64::from_le_bytes(buf))
}

pub(crate) fn insert_u32(current: u32, offset: u64, size: u8, value: u64) -> u32 {
    let shift = u32::try_from(offset).unwrap_or(0) * 8;
    let width_mask: u32 = match size {
        1 => 0xff,
        2 => 0xffff,
        4 => 0xffff_ffff,
        _ => 0xffff_ffff,
    };
    let field_mask = width_mask.checked_shl(shift).unwrap_or(0);
    let placed = ((value as u32) & width_mask)
        .checked_shl(shift)
        .unwrap_or(0);
    (current & !field_mask) | placed
}

pub(crate) fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
}

/// Whether the env-gated control-plane trace is on. Read once; when off the
/// per-event trace sites collapse to a single cached bool check.
pub(crate) fn console_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_VIRTIO_CONSOLE_TRACE")
                .as_deref()
                .map(str::trim),
            Ok("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
        )
    })
}

//! Split out of virtio_net.rs to keep files under 850 lines.

use super::*;

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
    pcie::{
        VIRTIO_NET_MSIX_PBA_OFFSET, VIRTIO_NET_MSIX_TABLE_OFFSET, VIRTIO_NET_MSIX_VECTOR_COUNT,
    },
};

#[derive(Debug)]
pub struct VirtioPciNet<B: NetBackend = LoopbackTestBackend> {
    pub(crate) net: VirtioNet<B>,
    pub(crate) msix: MsixTable,
}

impl VirtioPciNet<LoopbackTestBackend> {
    pub fn new_loopback() -> Self {
        Self::new(LoopbackTestBackend::default())
    }
}

impl<B: NetBackend> VirtioPciNet<B> {
    pub fn new(backend: B) -> Self {
        Self {
            net: VirtioNet::new(backend),
            msix: MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT),
        }
    }

    pub fn backend(&self) -> &B {
        self.net.backend()
    }

    pub fn backend_mut(&mut self) -> &mut B {
        self.net.backend_mut()
    }

    pub fn stats(&self) -> VirtioNetStats {
        self.net.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.net.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.net.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT);
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.net.pump_receive(mem)
    }

    pub fn poll_host_sockets(&mut self) {
        self.net.backend_mut().poll_host_sockets();
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciNetOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioNetResult {
        let is_write = matches!(op, VirtioPciNetOp::Write { .. });
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    self.net.access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.net
                        .access_common(common_offset, true, size, value, mem)
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.net.config_read(device_offset, size))
                }
                VirtioPciNetOp::Write { .. } => VirtioNetResult::WriteAck,
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciNetOp::Read { .. } => VirtioNetResult::ReadValue(0),
                VirtioPciNetOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.net.notify_queue(queue, mem);
                    VirtioNetResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciNetOp::Read { size } => VirtioNetResult::ReadValue(mask_to_size(
                    u64::from(self.net.interrupt_status),
                    size,
                )),
                VirtioPciNetOp::Write { value, .. } => {
                    self.net.interrupt_status &= !(value as u32);
                    VirtioNetResult::WriteAck
                }
            };
        }
        match (op, is_write) {
            (VirtioPciNetOp::Read { .. }, _) => VirtioNetResult::ReadValue(0),
            (VirtioPciNetOp::Write { .. }, _) => VirtioNetResult::WriteAck,
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciNetOp) -> VirtioNetResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioNetResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioNetResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciNetOp::Read { .. } => VirtioNetResult::ReadValue(0),
            VirtioPciNetOp::Write { .. } => VirtioNetResult::WriteAck,
        }
    }

    pub fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.raise_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let mut pending = self.net.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.net.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.net.queues[queue_index].pending_msix = false;
                self.net.pending_msix_queue_bits &= !(1u8 << queue_index);
                out.push(message);
            }
            pending &= !(1u8 << queue_index);
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

    pub(crate) fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.net.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.net.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_NET_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let net = &self.net;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_blob(&net.mac);
        out.write_u32(net.device_features_sel);
        out.write_u32(net.driver_features_sel);
        out.write_u32(net.driver_features[0]);
        out.write_u32(net.driver_features[1]);
        out.write_u16(net.config_msix_vector);
        out.write_u16(0);
        out.write_u32(net.queue_sel);
        out.write_u8(net.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(net.status);
        out.write_u32(net.interrupt_status);

        for queue in &net.queues {
            out.write_u16(queue.size);
            out.write_bool(queue.ready);
            out.write_bool(queue.pending_msix);
            out.write_u64(queue.desc);
            out.write_u64(queue.driver);
            out.write_u64(queue.device);
            out.write_u16(queue.msix_vector);
            out.write_u16(queue.notify_off);
            out.write_u16(queue.last_avail_idx);
            out.write_u16(0);
        }

        out.write_bool(net.pending_rx_frame.is_some());
        if let Some(frame) = &net.pending_rx_frame {
            out.write_blob(frame);
        }

        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-net snapshot version"
        );

        let mac = input.read_blob();
        assert_eq!(mac.len(), 6, "invalid restored virtio-net MAC");
        self.net.mac.copy_from_slice(&mac);
        self.net.device_features_sel = input.read_u32();
        self.net.driver_features_sel = input.read_u32();
        self.net.driver_features = [input.read_u32(), input.read_u32()];
        self.net.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-net snapshot");
        self.net.queue_sel = input.read_u32();
        self.net.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-net snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-net snapshot");
        self.net.status = input.read_u32();
        self.net.interrupt_status = input.read_u32();

        for queue in &mut self.net.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid virtio-net queue snapshot");
        }

        self.net.pending_rx_frame = if input.read_bool() {
            Some(input.read_blob())
        } else {
            None
        };
        self.net.descriptor_scratch.clear();
        self.net.tx_packet_scratch.clear();
        self.net.rx_frame_scratch.clear();

        self.msix.restore_state(&input.read_blob());
        input.finish();
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

pub(crate) fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

pub(crate) fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

pub(crate) fn offered_features_word(select: u32) -> u32 {
    match select {
        0 => VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS,
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

pub(crate) fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
}

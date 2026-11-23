//! The modern virtio-PCI transport shared by virtio-console, virtio-net and
//! virtio-gpu: common-config registers (and their virtio-MMIO aliases), the
//! ISR, notify and device-config windows, and per-queue MSI-X bookkeeping.
//!
//! A device keeps a [`VirtioPciState`] and implements [`VirtioPciDevice`]:
//! its id, queue limit, offered features, device-config window, doorbell and
//! reset. The remaining hooks have no-op defaults; only virtio-gpu overrides
//! them (MSI-X vector validation, the config-change vector, fence draining
//! after a common write and its trace output).

mod registers;
#[cfg(test)]
mod tests;

pub(crate) use registers::*;

use crate::fwcfg::GuestMemoryMut;
use crate::msix::{MsixMessage, MsixTable};
use crate::virtio_queue::clamp_u16;

/// Result of one BAR access; each device re-exports it under its own name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioPciResult {
    ReadValue(u64),
    WriteAck,
}

/// One BAR access; each device re-exports it under its own name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

/// Driver-programmed registers of one virtqueue. `C` carries per-device extras
/// (virtio-console's delivery counters); the other devices use `()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtioPciQueue<C = ()> {
    pub(crate) size: u16,
    pub(crate) ready: bool,
    pub(crate) desc: u64,
    pub(crate) driver: u64,
    pub(crate) device: u64,
    pub(crate) msix_vector: u16,
    pub(crate) notify_off: u16,
    pub(crate) last_avail_idx: u16,
    pub(crate) pending_msix: bool,
    pub(crate) counters: C,
}

impl<C: Default> VirtioPciQueue<C> {
    pub(crate) fn new(notify_off: u16) -> Self {
        Self {
            size: 0,
            ready: false,
            desc: 0,
            driver: 0,
            device: 0,
            msix_vector: VIRTIO_MSI_NO_VECTOR,
            notify_off,
            last_avail_idx: 0,
            pending_msix: false,
            counters: C::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        let notify_off = self.notify_off;
        *self = Self::new(notify_off);
    }
}

impl<C> VirtioPciQueue<C> {
    /// Queue size the device must actually run at. The virtio driver may enable
    /// a queue without ever writing COMMON_QUEUE_SIZE, in which case the queue
    /// operates at the advertised maximum rather than the reset value of 0.
    /// Reads of COMMON_QUEUE_SIZE report this effective value, so descriptor
    /// processing must agree with it.
    pub(crate) fn effective_size(&self, max: u16) -> u16 {
        if self.size == 0 {
            max
        } else {
            self.size
        }
    }
}

/// Transport state every virtio-PCI device keeps for its `N` queues.
#[derive(Debug)]
pub(crate) struct VirtioPciState<C, const N: usize> {
    pub(crate) device_features_sel: u32,
    pub(crate) driver_features_sel: u32,
    pub(crate) driver_features: [u32; 2],
    pub(crate) config_msix_vector: u16,
    pub(crate) queue_sel: u32,
    pub(crate) queues: [VirtioPciQueue<C>; N],
    pub(crate) pending_msix_queue_bits: u8,
    pub(crate) status: u32,
    pub(crate) interrupt_status: u32,
}

impl<C: Copy + Default, const N: usize> VirtioPciState<C, N> {
    /// Reset state; queue `i` gets notify offset `i`.
    pub(crate) fn new() -> Self {
        Self {
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            config_msix_vector: VIRTIO_MSI_NO_VECTOR,
            queue_sel: 0,
            queues: std::array::from_fn(|index| VirtioPciQueue::new(index as u16)),
            pending_msix_queue_bits: 0,
            status: 0,
            interrupt_status: 0,
        }
    }

    /// Device reset; each queue keeps its notify offset.
    pub(crate) fn reset(&mut self) {
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.driver_features = [0; 2];
        self.config_msix_vector = VIRTIO_MSI_NO_VECTOR;
        self.queue_sel = 0;
        for queue in &mut self.queues {
            queue.reset();
        }
        self.pending_msix_queue_bits = 0;
        self.status = 0;
        self.interrupt_status = 0;
    }

    /// Both negotiated feature words as one value.
    pub(crate) fn driver_features_u64(&self) -> u64 {
        u64::from(self.driver_features[0]) | (u64::from(self.driver_features[1]) << 32)
    }

    pub(crate) fn selected_queue(&self) -> Option<VirtioPciQueue<C>> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    pub(crate) fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioPciQueue<C>)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    pub(crate) fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
    }

    fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }
}

/// MSI-X table and PBA access within the device's MSI-X BAR.
pub(crate) fn msix_bar_access(
    msix: &mut MsixTable,
    table_base: u32,
    pba_base: u32,
    offset: u64,
    op: VirtioPciOp,
) -> VirtioPciResult {
    let within = |base: u32, len: u64| {
        let rel = offset.checked_sub(u64::from(base))?;
        (rel < len).then_some(rel)
    };
    if let Some(table_offset) = within(table_base, msix.table_byte_len()) {
        return match op {
            VirtioPciOp::Read { size } => {
                VirtioPciResult::ReadValue(msix.table_read(table_offset, size))
            }
            VirtioPciOp::Write { size, value } => {
                msix.table_write(table_offset, size, value);
                VirtioPciResult::WriteAck
            }
        };
    }
    if let Some(pba_offset) = within(pba_base, msix.pba_byte_len()) {
        return match op {
            VirtioPciOp::Read { size } => {
                VirtioPciResult::ReadValue(msix.pba_read(pba_offset, size))
            }
            VirtioPciOp::Write { size, value } => {
                msix.pba_write(pba_offset, size, value);
                VirtioPciResult::WriteAck
            }
        };
    }
    match op {
        VirtioPciOp::Read { .. } => VirtioPciResult::ReadValue(0),
        VirtioPciOp::Write { .. } => VirtioPciResult::WriteAck,
    }
}

/// A device on the shared transport. `C` and `N` are its queue extras and
/// queue count.
pub(crate) trait VirtioPciDevice<C: Copy + Default, const N: usize> {
    /// virtio device id reported through the MMIO alias.
    const DEVICE_ID: u32;
    /// Largest queue size; also the size reported for a queue never sized.
    const QUEUE_MAX: u16;

    fn pci(&self) -> &VirtioPciState<C, N>;
    fn pci_mut(&mut self) -> &mut VirtioPciState<C, N>;
    /// Feature word `select` the device offers.
    fn offered_features_word(&self, select: u32) -> u32;
    /// Device reset after the driver writes status 0.
    fn reset_device(&mut self);
    /// Queue doorbell from the notify window or the MMIO alias.
    fn notify(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut);
    /// Device-specific configuration window read.
    fn config_read(&self, offset: u64, size: u8) -> u64;
    /// Device-specific configuration window write; ignored by default.
    fn config_write(&mut self, _offset: u64, _size: u8, _value: u64) {}

    // Hooks only virtio-gpu overrides; the defaults change nothing.

    /// Vector stored when the driver programs an MSI-X vector.
    fn accept_msix_vector(&self, vector: u16) -> u16 {
        vector
    }
    fn after_common_read(&mut self, _offset: u64, _size: u8, _value: u64) {}
    fn after_common_write(&mut self, _mem: &mut dyn GuestMemoryMut) {}
    fn before_status_write(&mut self, _value: u64) {}
    fn after_driver_features_write(&mut self, _value: u64) {}
    /// `queue` is `None` for the config vector.
    fn after_msix_vector_write(&self, _queue: Option<u32>, _raw: u16) {}
    fn after_queue_enable_write(&self, _enable: u64) {}
    /// Raised before the queue vectors on every MSI-X raise.
    fn raise_config_msix(
        &mut self,
        _msix: &mut MsixTable,
        _function_enabled: bool,
        _function_masked: bool,
        _out: &mut Vec<MsixMessage>,
    ) {
    }
    /// A pending vector the MSI-X table delivered.
    fn msix_delivered(&mut self, _vector: u16) {}
    fn trace_queue_msix(&self, _what: &str, _queue_index: usize, _vector: u16) {}

    /// Access to the common, ISR, device-config and notify windows.
    fn pci_access(
        &mut self,
        offset: u64,
        op: VirtioPciOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioPciResult {
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciOp::Read { size } => {
                    self.access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciOp::Write { size, value } => {
                    let result = self.access_common(common_offset, true, size, value, mem);
                    self.after_common_write(mem);
                    result
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciOp::Read { size } => {
                    VirtioPciResult::ReadValue(self.config_read(device_offset, size))
                }
                VirtioPciOp::Write { size, value } => {
                    self.config_write(device_offset, size, value);
                    VirtioPciResult::WriteAck
                }
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciOp::Read { .. } => VirtioPciResult::ReadValue(0),
                VirtioPciOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.notify(queue, mem);
                    VirtioPciResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            let interrupt_status = &mut self.pci_mut().interrupt_status;
            return match op {
                VirtioPciOp::Read { size } => VirtioPciResult::ReadValue(mask_to_size(
                    u64::from(std::mem::take(interrupt_status)),
                    size,
                )),
                VirtioPciOp::Write { value, .. } => {
                    *interrupt_status &= !(value as u32);
                    VirtioPciResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciOp::Read { .. } => VirtioPciResult::ReadValue(0),
            VirtioPciOp::Write { .. } => VirtioPciResult::WriteAck,
        }
    }

    fn access_common(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioPciResult {
        if !is_write {
            let value = self.read_common(offset, size);
            self.after_common_read(offset, size, value);
            return VirtioPciResult::ReadValue(value);
        }
        self.write_common(offset, size, value, mem);
        VirtioPciResult::WriteAck
    }

    fn read_common(&self, offset: u64, size: u8) -> u64 {
        if let Some(value) = self.read_common_field(offset, size) {
            return value;
        }
        self.read_mmio_alias(offset, size)
    }

    fn read_mmio_alias(&self, offset: u64, size: u8) -> u64 {
        let pci = self.pci();
        let queue = pci.selected_queue();
        let value = match offset {
            REG_MAGIC => u64::from(MAGIC_VALUE),
            REG_VERSION => u64::from(VERSION_MODERN),
            REG_DEVICE_ID => u64::from(Self::DEVICE_ID),
            REG_VENDOR_ID => u64::from(VENDOR_ID_COMPAT),
            REG_DEVICE_FEATURES => u64::from(self.offered_features_word(pci.device_features_sel)),
            REG_DRIVER_FEATURES => {
                u64::from(pci.driver_features[pci.driver_features_sel.min(1) as usize])
            }
            REG_QUEUE_NUM_MAX => {
                if queue.is_some() {
                    u64::from(Self::QUEUE_MAX)
                } else {
                    0
                }
            }
            REG_QUEUE_NUM => queue.map_or(0, |q| u64::from(q.size)),
            REG_QUEUE_READY => queue.map_or(0, |q| u64::from(q.ready as u8)),
            REG_INTERRUPT_STATUS => u64::from(pci.interrupt_status),
            REG_STATUS => u64::from(pci.status),
            REG_QUEUE_DESC_LOW => queue.map_or(0, |q| q.desc & 0xffff_ffff),
            REG_QUEUE_DESC_HIGH => queue.map_or(0, |q| q.desc >> 32),
            REG_QUEUE_DRIVER_LOW => queue.map_or(0, |q| q.driver & 0xffff_ffff),
            REG_QUEUE_DRIVER_HIGH => queue.map_or(0, |q| q.driver >> 32),
            REG_QUEUE_DEVICE_LOW => queue.map_or(0, |q| q.device & 0xffff_ffff),
            REG_QUEUE_DEVICE_HIGH => queue.map_or(0, |q| q.device >> 32),
            REG_CONFIG_GENERATION => 0,
            _ => 0,
        };
        mask_to_size(value, size)
    }

    fn write_common(&mut self, offset: u64, size: u8, value: u64, mem: &mut dyn GuestMemoryMut) {
        if self.write_common_field(offset, size, value) {
            return;
        }
        self.write_mmio_alias(offset, value, mem);
    }

    fn write_mmio_alias(&mut self, offset: u64, value: u64, mem: &mut dyn GuestMemoryMut) {
        let queue_max = Self::QUEUE_MAX;
        let pci = self.pci_mut();
        match offset {
            REG_DEVICE_FEATURES_SEL => pci.device_features_sel = value as u32,
            REG_DRIVER_FEATURES_SEL => pci.driver_features_sel = value as u32,
            REG_DRIVER_FEATURES => self.write_driver_features(value),
            REG_QUEUE_SEL => pci.queue_sel = value as u32,
            REG_QUEUE_NUM => pci.write_selected_queue(|q| q.size = clamp_u16(value, queue_max)),
            REG_QUEUE_READY => pci.write_selected_queue(|q| {
                q.ready = value != 0;
                if !q.ready {
                    q.last_avail_idx = 0;
                }
            }),
            REG_QUEUE_NOTIFY => self.notify(value as u16, mem),
            REG_INTERRUPT_ACK => pci.interrupt_status &= !(value as u32),
            REG_STATUS => self.write_status(value),
            REG_QUEUE_DESC_LOW => pci.write_selected_queue(|q| q.desc = set_low(q.desc, value)),
            REG_QUEUE_DESC_HIGH => pci.write_selected_queue(|q| q.desc = set_high(q.desc, value)),
            REG_QUEUE_DRIVER_LOW => {
                pci.write_selected_queue(|q| q.driver = set_low(q.driver, value))
            }
            REG_QUEUE_DRIVER_HIGH => {
                pci.write_selected_queue(|q| q.driver = set_high(q.driver, value))
            }
            REG_QUEUE_DEVICE_LOW => {
                pci.write_selected_queue(|q| q.device = set_low(q.device, value))
            }
            REG_QUEUE_DEVICE_HIGH => {
                pci.write_selected_queue(|q| q.device = set_high(q.device, value))
            }
            _ => {}
        }
    }

    fn write_driver_features(&mut self, value: u64) {
        let select = self.pci().driver_features_sel;
        if select < 2 {
            let offered = self.offered_features_word(select);
            self.pci_mut().driver_features[select as usize] = (value as u32) & offered;
            self.after_driver_features_write(value);
        }
    }

    fn read_common_field(&self, offset: u64, size: u8) -> Option<u64> {
        if !is_supported_common_access_size(size) {
            return None;
        }
        let pci = self.pci();
        let selected_queue = pci.selected_queue();
        let fields = [
            (
                COMMON_DEVICE_FEATURE_SELECT,
                4,
                u64::from(pci.device_features_sel),
            ),
            (
                COMMON_DEVICE_FEATURE,
                4,
                u64::from(self.offered_features_word(pci.device_features_sel)),
            ),
            (
                COMMON_DRIVER_FEATURE_SELECT,
                4,
                u64::from(pci.driver_features_sel),
            ),
            (
                COMMON_DRIVER_FEATURE,
                4,
                u64::from(pci.driver_features[pci.driver_features_sel.min(1) as usize]),
            ),
            (
                COMMON_CONFIG_MSIX_VECTOR,
                2,
                u64::from(pci.config_msix_vector),
            ),
            (COMMON_NUM_QUEUES, 2, N as u64),
            (COMMON_DEVICE_STATUS, 1, u64::from(pci.status & 0xff)),
            (COMMON_CONFIG_GENERATION, 1, 0),
            (COMMON_QUEUE_SELECT, 2, u64::from(pci.queue_sel as u16)),
            (
                COMMON_QUEUE_SIZE,
                2,
                selected_queue.map_or(0, |q| u64::from(q.effective_size(Self::QUEUE_MAX))),
            ),
            (
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                selected_queue.map_or(u64::from(VIRTIO_MSI_NO_VECTOR), |q| {
                    u64::from(q.msix_vector)
                }),
            ),
            (
                COMMON_QUEUE_ENABLE,
                2,
                selected_queue.map_or(0, |q| u64::from(q.ready as u8)),
            ),
            (
                COMMON_QUEUE_NOTIFY_OFF,
                2,
                selected_queue.map_or(0, |q| u64::from(q.notify_off)),
            ),
            (COMMON_QUEUE_DESC, 8, selected_queue.map_or(0, |q| q.desc)),
            (
                COMMON_QUEUE_DRIVER,
                8,
                selected_queue.map_or(0, |q| q.driver),
            ),
            (
                COMMON_QUEUE_DEVICE,
                8,
                selected_queue.map_or(0, |q| q.device),
            ),
        ];
        fields.iter().find_map(|(base, width, value)| {
            read_common_register(*base, *width, *value, offset, size)
        })
    }

    fn write_common_field(&mut self, offset: u64, size: u8, value: u64) -> bool {
        if !is_supported_common_access_size(size) {
            return false;
        }
        let merge = |current: u64, base: u64, width: u8| {
            write_common_register(current, base, width, offset, size, value)
        };
        let pci = self.pci_mut();
        if common_access_touches(COMMON_DEVICE_FEATURE_SELECT, 4, offset, size) {
            pci.device_features_sel = merge(
                pci.device_features_sel.into(),
                COMMON_DEVICE_FEATURE_SELECT,
                4,
            ) as u32;
            return true;
        }
        if common_access_touches(COMMON_DRIVER_FEATURE_SELECT, 4, offset, size) {
            pci.driver_features_sel = merge(
                pci.driver_features_sel.into(),
                COMMON_DRIVER_FEATURE_SELECT,
                4,
            ) as u32;
            return true;
        }
        if common_access_touches(COMMON_DRIVER_FEATURE, 4, offset, size) {
            let current = pci.driver_features[pci.driver_features_sel.min(1) as usize];
            self.write_driver_features(merge(current.into(), COMMON_DRIVER_FEATURE, 4));
            return true;
        }
        if common_access_touches(COMMON_CONFIG_MSIX_VECTOR, 2, offset, size) {
            let vector = merge(pci.config_msix_vector.into(), COMMON_CONFIG_MSIX_VECTOR, 2) as u16;
            self.pci_mut().config_msix_vector = self.accept_msix_vector(vector);
            self.after_msix_vector_write(None, vector);
            return true;
        }
        if common_access_touches(COMMON_DEVICE_STATUS, 1, offset, size) {
            let status = merge(u64::from(pci.status & 0xff), COMMON_DEVICE_STATUS, 1);
            self.write_status(status);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_SELECT, 2, offset, size) {
            pci.queue_sel = merge(u64::from(pci.queue_sel as u16), COMMON_QUEUE_SELECT, 2) as u32;
            return true;
        }
        let queue_sel = pci.queue_sel;
        let Some(queue) = pci.queues.get_mut(queue_sel as usize) else {
            return common_access_touches_queue_field(offset, size);
        };
        if common_access_touches(COMMON_QUEUE_SIZE, 2, offset, size) {
            queue.size =
                (merge(u64::from(queue.size), COMMON_QUEUE_SIZE, 2) as u16).min(Self::QUEUE_MAX);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_MSIX_VECTOR, 2, offset, size) {
            let vector = merge(u64::from(queue.msix_vector), COMMON_QUEUE_MSIX_VECTOR, 2) as u16;
            let accepted = self.accept_msix_vector(vector);
            self.pci_mut().queues[queue_sel as usize].msix_vector = accepted;
            self.after_msix_vector_write(Some(queue_sel), vector);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_ENABLE, 2, offset, size) {
            let enable = merge(u64::from(queue.ready as u8), COMMON_QUEUE_ENABLE, 2);
            queue.ready = enable == 1;
            if !queue.ready {
                queue.last_avail_idx = 0;
            }
            self.after_queue_enable_write(enable);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DESC, 8, offset, size) {
            queue.desc = merge(queue.desc, COMMON_QUEUE_DESC, 8);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DRIVER, 8, offset, size) {
            queue.driver = merge(queue.driver, COMMON_QUEUE_DRIVER, 8);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DEVICE, 8, offset, size) {
            queue.device = merge(queue.device, COMMON_QUEUE_DEVICE, 8);
            return true;
        }
        false
    }

    fn write_status(&mut self, value: u64) {
        self.before_status_write(value);
        self.pci_mut().status = value as u32;
        if value == 0 {
            self.reset_device();
        }
    }

    /// Raise every queue vector with a pending interrupt (after the device's
    /// config-change vector, if any).
    fn raise_msix_into(
        &mut self,
        msix: &mut MsixTable,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.raise_config_msix(msix, function_enabled, function_masked, out);
        let mut pending = self.pci().pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.pci().queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                self.trace_queue_msix("no-vector (ISR path)", queue_index, vector);
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = msix.raise(vector, function_enabled, function_masked) {
                let pci = self.pci_mut();
                pci.queues[queue_index].pending_msix = false;
                pci.pending_msix_queue_bits &= !(1u8 << queue_index);
                self.trace_queue_msix("raised", queue_index, vector);
                out.push(message);
            } else {
                self.trace_queue_msix("held (disabled/masked)", queue_index, vector);
            }
            pending &= !(1u8 << queue_index);
        }
    }

    /// Deliver vectors the MSI-X table held while masked, then raise new ones.
    fn drain_msix_into(
        &mut self,
        msix: &mut MsixTable,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let start = out.len();
        msix.drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.msix_delivered(message.vector);
            self.pci_mut()
                .clear_pending_queue_for_vector(message.vector);
        }
        self.raise_msix_into(msix, function_enabled, function_masked, out);
    }
}

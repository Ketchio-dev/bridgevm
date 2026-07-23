//! Split out of virtio_blk.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_VECTOR_COUNT;
use std::io;
use std::path::Path;
use trace::RecentVirtioBlockRequests;

pub const INSTALLER_ISO_SLOT: u64 = machine::VIRTIO_MMIO_COUNT - 1;

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976; // "virt"
pub(crate) const VERSION_LEGACY: u32 = 1;
pub(crate) const VERSION_MODERN: u32 = 2;
pub(crate) const DEVICE_ID_BLOCK: u32 = 2;
pub(crate) const VENDOR_ID_QEMU: u32 = 0x554d_4551; // "QEMU"

pub(crate) const REG_MAGIC: u64 = 0x000;
pub(crate) const REG_VERSION: u64 = 0x004;
pub(crate) const REG_DEVICE_ID: u64 = 0x008;
pub(crate) const REG_VENDOR_ID: u64 = 0x00c;
pub(crate) const REG_DEVICE_FEATURES: u64 = 0x010;
pub(crate) const REG_DEVICE_FEATURES_SEL: u64 = 0x014;
pub(crate) const REG_DRIVER_FEATURES: u64 = 0x020;
pub(crate) const REG_DRIVER_FEATURES_SEL: u64 = 0x024;
pub(crate) const REG_GUEST_PAGE_SIZE: u64 = 0x028;
pub(crate) const REG_QUEUE_SEL: u64 = 0x030;
pub(crate) const REG_QUEUE_NUM_MAX: u64 = 0x034;
pub(crate) const REG_QUEUE_NUM: u64 = 0x038;
pub(crate) const REG_QUEUE_ALIGN: u64 = 0x03c;
pub(crate) const REG_QUEUE_PFN: u64 = 0x040;
pub(crate) const REG_QUEUE_READY: u64 = 0x044;
pub(crate) const REG_QUEUE_NOTIFY: u64 = 0x050;
pub(crate) const REG_INTERRUPT_STATUS: u64 = 0x060;
pub(crate) const REG_INTERRUPT_ACK: u64 = 0x064;
pub(crate) const REG_STATUS: u64 = 0x070;
pub(crate) const REG_QUEUE_DESC_LOW: u64 = 0x080;
pub(crate) const REG_QUEUE_DESC_HIGH: u64 = 0x084;
pub(crate) const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
pub(crate) const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
pub(crate) const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
pub(crate) const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
pub(crate) const REG_CONFIG_GENERATION: u64 = 0x0fc;
pub(crate) const REG_CONFIG: u64 = 0x100;

pub(crate) const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;
pub(crate) const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
pub(crate) const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;
pub(crate) const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
pub(crate) const PCI_CFG_REGION_SIZE: u64 = 0x1000;

pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0; // bit 32, selected through features_sel=1
pub(crate) const VIRTIO_BLK_F_RO: u32 = 1 << 5;
pub(crate) const VIRTIO_BLK_F_BLK_SIZE: u32 = 1 << 6;

pub(crate) const QUEUE_MAX: u16 = 128;
pub(crate) const SECTOR_SIZE: u64 = 512;
pub(crate) const DESC_SIZE: u64 = 16;
pub(crate) const DESC_F_NEXT: u16 = 1;
pub(crate) const DESC_F_WRITE: u16 = 2;
pub(crate) const READ_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) const VIRTIO_BLK_T_IN: u32 = 0;
pub(crate) const VIRTIO_BLK_S_OK: u8 = 0;
pub(crate) const VIRTIO_BLK_S_IOERR: u8 = 1;
pub(crate) const VIRTIO_BLK_S_UNSUPP: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VirtioMmioTransport {
    Legacy,
    Modern,
}

impl VirtioMmioTransport {
    pub(crate) const fn version(self) -> u32 {
        match self {
            Self::Legacy => VERSION_LEGACY,
            Self::Modern => VERSION_MODERN,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioMmioBlockResult {
    ReadValue(u64),
    WriteAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciBlockOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

#[derive(Debug)]
pub struct VirtioMmioBlock {
    pub(crate) backend: RawFileBackend,
    pub(crate) stats: VirtioMmioBlockStats,
    pub(crate) transport: VirtioMmioTransport,
    pub(crate) device_features_sel: u32,
    pub(crate) driver_features_sel: u32,
    pub(crate) driver_features: [u32; 2],
    pub(crate) guest_page_size: u32,
    pub(crate) queue_sel: u32,
    pub(crate) queue_num: u16,
    pub(crate) queue_align: u32,
    pub(crate) queue_ready: bool,
    pub(crate) queue_desc: u64,
    pub(crate) queue_driver: u64,
    pub(crate) queue_device: u64,
    pub(crate) status: u32,
    pub(crate) interrupt_status: u32,
    pub(crate) last_avail_idx: u16,
    pub(crate) request_sequence: u64,
    pub(crate) request_trace: RecentVirtioBlockRequests,
    pub(crate) descriptor_scratch: Vec<Descriptor>,
    pub(crate) read_scratch: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VirtioMmioBlockStats {
    pub transport_version: u32,
    pub notify_count: u64,
    pub request_count: u64,
    pub read_count: u64,
    pub unsupported_count: u64,
    pub io_error_count: u64,
    pub bytes_read: u64,
    pub last_sector: Option<u64>,
    pub last_len: u32,
    pub last_status: Option<u8>,
    pub queue_num: u16,
    pub queue_ready: bool,
    pub queue_desc: u64,
    pub queue_driver: u64,
    pub queue_device: u64,
    pub status: u32,
    pub driver_features: u64,
}

impl VirtioMmioBlock {
    pub fn open_read_only(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_read_only_with_transport(path, VirtioMmioTransport::Legacy)
    }

    pub(crate) fn open_read_only_modern(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_read_only_with_transport(path, VirtioMmioTransport::Modern)
    }

    pub(crate) fn open_read_only_with_transport(
        path: impl AsRef<Path>,
        transport: VirtioMmioTransport,
    ) -> io::Result<Self> {
        Ok(Self {
            backend: RawFileBackend::open(path)?,
            stats: VirtioMmioBlockStats::default(),
            transport,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            guest_page_size: 4096,
            queue_sel: 0,
            queue_num: 0,
            queue_align: 4096,
            queue_ready: false,
            queue_desc: 0,
            queue_driver: 0,
            queue_device: 0,
            status: 0,
            interrupt_status: 0,
            last_avail_idx: 0,
            request_sequence: 0,
            request_trace: RecentVirtioBlockRequests::default(),
            descriptor_scratch: Vec::new(),
            read_scratch: Vec::new(),
        })
    }

    pub fn len(&self) -> u64 {
        self.backend.len
    }

    pub fn is_empty(&self) -> bool {
        self.backend.len == 0
    }

    pub fn stats(&self) -> VirtioMmioBlockStats {
        let mut stats = self.stats;
        stats.transport_version = self.transport.version();
        stats.queue_num = self.queue_num;
        stats.queue_ready = self.queue_ready;
        stats.queue_desc = self.queue_desc;
        stats.queue_driver = self.queue_driver;
        stats.queue_device = self.queue_device;
        stats.status = self.status;
        stats.driver_features =
            u64::from(self.driver_features[0]) | (u64::from(self.driver_features[1]) << 32);
        stats
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.interrupt_status != 0
    }

    pub fn recent_request_trace(&self) -> Vec<VirtioBlockRequestTrace> {
        self.request_trace.snapshot()
    }

    pub fn reset_runtime_state(&mut self) {
        self.stats = VirtioMmioBlockStats::default();
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.driver_features = [0; 2];
        self.guest_page_size = 4096;
        self.queue_sel = 0;
        self.queue_num = 0;
        self.queue_align = 4096;
        self.queue_ready = false;
        self.queue_desc = 0;
        self.queue_driver = 0;
        self.queue_device = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.last_avail_idx = 0;
        self.request_sequence = 0;
        self.request_trace = RecentVirtioBlockRequests::default();
        self.descriptor_scratch.clear();
        self.read_scratch.clear();
    }

    pub fn access(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioMmioBlockResult {
        if !is_write {
            return VirtioMmioBlockResult::ReadValue(self.read(offset, size));
        }
        self.write(offset, size, value, mem);
        VirtioMmioBlockResult::WriteAck
    }

    pub(crate) fn legacy_pci_io_read(&self, offset: u64, size: u8) -> u64 {
        let value = match offset {
            0x00 => u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE),
            0x04 => u64::from(self.driver_features[0]),
            0x08 => {
                if self.queue_ready && self.guest_page_size != 0 {
                    self.queue_desc / u64::from(self.guest_page_size)
                } else {
                    0
                }
            }
            0x0c => {
                if self.queue_sel == 0 {
                    u64::from(QUEUE_MAX)
                } else {
                    0
                }
            }
            0x0e => u64::from(self.queue_sel),
            0x12 => u64::from(self.status),
            0x13 => u64::from(self.interrupt_status),
            o if o >= 0x14 => self.config_read(o - 0x14, size),
            _ => 0,
        };
        mask_to_size(value, size)
    }

    pub(crate) fn legacy_pci_io_write(
        &mut self,
        offset: u64,
        _size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        match offset {
            0x04 => self.driver_features[0] = value as u32,
            0x08 => {
                if self.queue_sel == 0 && value != 0 {
                    self.guest_page_size = 4096;
                    self.queue_num = QUEUE_MAX;
                    self.queue_desc = value.saturating_mul(u64::from(self.guest_page_size));
                    self.queue_ready = true;
                    self.derive_legacy_queue_addresses();
                } else if self.queue_sel == 0 {
                    self.queue_ready = false;
                    self.last_avail_idx = 0;
                }
            }
            0x0e => self.queue_sel = value as u32,
            0x10 => {
                if value == 0 {
                    self.stats.notify_count = self.stats.notify_count.saturating_add(1);
                    self.process_queue(mem);
                }
            }
            0x12 => {
                self.status = value as u32;
                if value == 0 {
                    self.reset_queue();
                }
            }
            0x13 => self.interrupt_status &= !(value as u32),
            _ => {}
        }
    }

    pub(crate) fn read(&self, offset: u64, size: u8) -> u64 {
        let value = match offset {
            REG_MAGIC => u64::from(MAGIC_VALUE),
            REG_VERSION => u64::from(self.transport.version()),
            REG_DEVICE_ID => u64::from(DEVICE_ID_BLOCK),
            REG_VENDOR_ID => u64::from(VENDOR_ID_QEMU),
            REG_DEVICE_FEATURES => u64::from(self.device_features()),
            REG_DRIVER_FEATURES => {
                u64::from(self.driver_features[self.driver_features_sel.min(1) as usize])
            }
            REG_QUEUE_NUM_MAX => {
                if self.queue_sel == 0 {
                    u64::from(QUEUE_MAX)
                } else {
                    0
                }
            }
            REG_QUEUE_NUM => u64::from(self.queue_num),
            REG_QUEUE_READY => u64::from(self.queue_ready as u32),
            REG_QUEUE_PFN if self.transport == VirtioMmioTransport::Legacy => {
                if self.queue_ready && self.guest_page_size != 0 {
                    self.queue_desc / u64::from(self.guest_page_size)
                } else {
                    0
                }
            }
            REG_INTERRUPT_STATUS => u64::from(self.interrupt_status),
            REG_STATUS => u64::from(self.status),
            REG_QUEUE_DESC_LOW => self.queue_desc & 0xffff_ffff,
            REG_QUEUE_DESC_HIGH => self.queue_desc >> 32,
            REG_QUEUE_DRIVER_LOW => self.queue_driver & 0xffff_ffff,
            REG_QUEUE_DRIVER_HIGH => self.queue_driver >> 32,
            REG_QUEUE_DEVICE_LOW => self.queue_device & 0xffff_ffff,
            REG_QUEUE_DEVICE_HIGH => self.queue_device >> 32,
            REG_CONFIG_GENERATION => 0,
            o if o >= REG_CONFIG => self.config_read(o - REG_CONFIG, size),
            _ => 0,
        };
        mask_to_size(value, size)
    }
}

#[derive(Debug)]
pub struct VirtioPciBlock {
    pub(crate) block: VirtioMmioBlock,
    pub(crate) msix: MsixTable,
}

impl VirtioPciBlock {
    pub fn open_read_only(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            block: VirtioMmioBlock::open_read_only_modern(path)?,
            msix: MsixTable::new(VIRTIO_BLK_MSIX_VECTOR_COUNT),
        })
    }

    pub fn stats(&self) -> VirtioMmioBlockStats {
        self.block.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.block.interrupt_line_level()
    }

    pub fn recent_request_trace(&self) -> Vec<VirtioBlockRequestTrace> {
        self.block.recent_request_trace()
    }

    pub fn reset_runtime_state(&mut self) {
        self.block.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_BLK_MSIX_VECTOR_COUNT);
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciBlockOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioMmioBlockResult {
        let is_write = matches!(op, VirtioPciBlockOp::Write { .. });
        let Some(mmio_offset) = pci_to_mmio_offset(offset, is_write) else {
            return match op {
                VirtioPciBlockOp::Read { .. } => VirtioMmioBlockResult::ReadValue(0),
                VirtioPciBlockOp::Write { .. } => VirtioMmioBlockResult::WriteAck,
            };
        };
        match op {
            VirtioPciBlockOp::Read { size } => self.block.access(mmio_offset, false, size, 0, mem),
            VirtioPciBlockOp::Write { size, value } => {
                self.block.access(mmio_offset, true, size, value, mem)
            }
        }
    }

    pub fn legacy_io_access(
        &mut self,
        offset: u64,
        op: VirtioPciBlockOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioMmioBlockResult {
        match op {
            VirtioPciBlockOp::Read { size } => {
                VirtioMmioBlockResult::ReadValue(self.block.legacy_pci_io_read(offset, size))
            }
            VirtioPciBlockOp::Write { size, value } => {
                self.block.legacy_pci_io_write(offset, size, value, mem);
                VirtioMmioBlockResult::WriteAck
            }
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciBlockOp) -> VirtioMmioBlockResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciBlockOp::Read { size } => {
                    VirtioMmioBlockResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciBlockOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioMmioBlockResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciBlockOp::Read { size } => {
                    VirtioMmioBlockResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciBlockOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioMmioBlockResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciBlockOp::Read { .. } => VirtioMmioBlockResult::ReadValue(0),
            VirtioPciBlockOp::Write { .. } => VirtioMmioBlockResult::WriteAck,
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_BLK_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_BLK_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_blob(&self.block.snapshot_state());
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-pci block snapshot version"
        );
        self.block.restore_state(&input.read_blob());
        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}

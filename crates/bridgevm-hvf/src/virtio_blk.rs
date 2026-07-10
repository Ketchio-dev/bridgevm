//! Minimal virtio-mmio block device for installer ISO media.
//!
//! QEMU's `virt` machine exposes 32 virtio-mmio transports. ArmVirtQemu's
//! firmware can boot a Windows ISO from a read-only virtio block device on one
//! of those transports and presents the El Torito image as `CDROM(0x0)`. This
//! module models just enough of QEMU's default legacy virtio-mmio transport and
//! split virtqueue block protocol for firmware reads from a host ISO file.

mod trace;

use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use crate::{
    fwcfg::GuestMemoryMut,
    machine,
    msix::MsixTable,
    pcie::{
        VIRTIO_BLK_MSIX_PBA_OFFSET, VIRTIO_BLK_MSIX_TABLE_OFFSET, VIRTIO_BLK_MSIX_VECTOR_COUNT,
    },
};
use trace::RecentVirtioBlockRequests;
pub use trace::{VirtioBlockRequestTrace, RECENT_REQUEST_TRACE_LIMIT};

pub const INSTALLER_ISO_SLOT: u64 = machine::VIRTIO_MMIO_COUNT - 1;

const MAGIC_VALUE: u32 = 0x7472_6976; // "virt"
const VERSION_LEGACY: u32 = 1;
const VERSION_MODERN: u32 = 2;
const DEVICE_ID_BLOCK: u32 = 2;
const VENDOR_ID_QEMU: u32 = 0x554d_4551; // "QEMU"

const REG_MAGIC: u64 = 0x000;
const REG_VERSION: u64 = 0x004;
const REG_DEVICE_ID: u64 = 0x008;
const REG_VENDOR_ID: u64 = 0x00c;
const REG_DEVICE_FEATURES: u64 = 0x010;
const REG_DEVICE_FEATURES_SEL: u64 = 0x014;
const REG_DRIVER_FEATURES: u64 = 0x020;
const REG_DRIVER_FEATURES_SEL: u64 = 0x024;
const REG_GUEST_PAGE_SIZE: u64 = 0x028;
const REG_QUEUE_SEL: u64 = 0x030;
const REG_QUEUE_NUM_MAX: u64 = 0x034;
const REG_QUEUE_NUM: u64 = 0x038;
const REG_QUEUE_ALIGN: u64 = 0x03c;
const REG_QUEUE_PFN: u64 = 0x040;
const REG_QUEUE_READY: u64 = 0x044;
const REG_QUEUE_NOTIFY: u64 = 0x050;
const REG_INTERRUPT_STATUS: u64 = 0x060;
const REG_INTERRUPT_ACK: u64 = 0x064;
const REG_STATUS: u64 = 0x070;
const REG_QUEUE_DESC_LOW: u64 = 0x080;
const REG_QUEUE_DESC_HIGH: u64 = 0x084;
const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
const REG_CONFIG_GENERATION: u64 = 0x0fc;
const REG_CONFIG: u64 = 0x100;

const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;
const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;
const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
const PCI_CFG_REGION_SIZE: u64 = 0x1000;

const VIRTIO_F_VERSION_1: u32 = 1 << 0; // bit 32, selected through features_sel=1
const VIRTIO_BLK_F_RO: u32 = 1 << 5;
const VIRTIO_BLK_F_BLK_SIZE: u32 = 1 << 6;

const QUEUE_MAX: u16 = 128;
const SECTOR_SIZE: u64 = 512;
const DESC_SIZE: u64 = 16;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const READ_CHUNK_BYTES: usize = 64 * 1024;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VirtioMmioTransport {
    Legacy,
    Modern,
}

impl VirtioMmioTransport {
    const fn version(self) -> u32 {
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
    backend: RawFileBackend,
    stats: VirtioMmioBlockStats,
    transport: VirtioMmioTransport,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: [u32; 2],
    guest_page_size: u32,
    queue_sel: u32,
    queue_num: u16,
    queue_align: u32,
    queue_ready: bool,
    queue_desc: u64,
    queue_driver: u64,
    queue_device: u64,
    status: u32,
    interrupt_status: u32,
    last_avail_idx: u16,
    request_sequence: u64,
    request_trace: RecentVirtioBlockRequests,
    descriptor_scratch: Vec<Descriptor>,
    read_scratch: Vec<u8>,
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

    fn open_read_only_modern(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_read_only_with_transport(path, VirtioMmioTransport::Modern)
    }

    fn open_read_only_with_transport(
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

    fn legacy_pci_io_read(&self, offset: u64, size: u8) -> u64 {
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

    fn legacy_pci_io_write(
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

    fn read(&self, offset: u64, size: u8) -> u64 {
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

    fn write(&mut self, offset: u64, _size: u8, value: u64, mem: &mut dyn GuestMemoryMut) {
        match offset {
            REG_DEVICE_FEATURES_SEL => self.device_features_sel = value as u32,
            REG_DRIVER_FEATURES_SEL => self.driver_features_sel = value as u32,
            REG_DRIVER_FEATURES => {
                if self.driver_features_sel < 2 {
                    self.driver_features[self.driver_features_sel as usize] = value as u32;
                }
            }
            REG_GUEST_PAGE_SIZE if self.transport == VirtioMmioTransport::Legacy => {
                if value != 0 {
                    self.guest_page_size = value as u32;
                }
            }
            REG_QUEUE_SEL => self.queue_sel = value as u32,
            REG_QUEUE_NUM => {
                if self.queue_sel == 0 {
                    self.queue_num = (value as u16).min(QUEUE_MAX);
                }
            }
            REG_QUEUE_ALIGN if self.transport == VirtioMmioTransport::Legacy => {
                if value != 0 {
                    self.queue_align = value as u32;
                    if self.queue_ready {
                        self.derive_legacy_queue_addresses();
                    }
                }
            }
            REG_QUEUE_PFN if self.transport == VirtioMmioTransport::Legacy => {
                if self.queue_sel == 0 && value != 0 {
                    self.queue_desc = value.saturating_mul(u64::from(self.guest_page_size));
                    self.queue_ready = true;
                    self.derive_legacy_queue_addresses();
                } else if self.queue_sel == 0 {
                    self.queue_ready = false;
                    self.last_avail_idx = 0;
                }
            }
            REG_QUEUE_READY => {
                if self.queue_sel == 0 && self.transport == VirtioMmioTransport::Modern {
                    self.queue_ready = value != 0;
                    if !self.queue_ready {
                        self.last_avail_idx = 0;
                    }
                }
            }
            REG_QUEUE_NOTIFY => {
                if value == 0 {
                    self.stats.notify_count = self.stats.notify_count.saturating_add(1);
                    self.process_queue(mem);
                }
            }
            REG_INTERRUPT_ACK => self.interrupt_status &= !(value as u32),
            REG_STATUS => {
                self.status = value as u32;
                if value == 0 {
                    self.reset_queue();
                }
            }
            REG_QUEUE_DESC_LOW if self.transport == VirtioMmioTransport::Modern => {
                self.queue_desc = set_low(self.queue_desc, value)
            }
            REG_QUEUE_DESC_HIGH if self.transport == VirtioMmioTransport::Modern => {
                self.queue_desc = set_high(self.queue_desc, value)
            }
            REG_QUEUE_DRIVER_LOW if self.transport == VirtioMmioTransport::Modern => {
                self.queue_driver = set_low(self.queue_driver, value)
            }
            REG_QUEUE_DRIVER_HIGH if self.transport == VirtioMmioTransport::Modern => {
                self.queue_driver = set_high(self.queue_driver, value)
            }
            REG_QUEUE_DEVICE_LOW if self.transport == VirtioMmioTransport::Modern => {
                self.queue_device = set_low(self.queue_device, value)
            }
            REG_QUEUE_DEVICE_HIGH if self.transport == VirtioMmioTransport::Modern => {
                self.queue_device = set_high(self.queue_device, value)
            }
            _ => {}
        }
    }

    fn reset_queue(&mut self) {
        self.queue_num = 0;
        self.queue_ready = false;
        self.queue_desc = 0;
        self.queue_driver = 0;
        self.queue_device = 0;
        self.queue_align = 4096;
        self.interrupt_status = 0;
        self.last_avail_idx = 0;
    }

    fn record_request_trace(&mut self, req_type: u32, sector: u64, data_len: u32, status: u8) {
        self.request_sequence = self.request_sequence.saturating_add(1);
        self.request_trace.record(VirtioBlockRequestTrace {
            sequence: self.request_sequence,
            request_type: req_type,
            sector,
            data_len,
            status,
        });
    }

    fn device_features(&self) -> u32 {
        if self.transport == VirtioMmioTransport::Legacy {
            return VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE;
        }
        match self.device_features_sel {
            0 => VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE,
            1 => VIRTIO_F_VERSION_1,
            _ => 0,
        }
    }

    fn derive_legacy_queue_addresses(&mut self) {
        if self.queue_num == 0 || self.queue_desc == 0 {
            return;
        }
        let avail = self
            .queue_desc
            .saturating_add(u64::from(self.queue_num) * DESC_SIZE);
        let used_unaligned = avail
            .saturating_add(4)
            .saturating_add(u64::from(self.queue_num) * 2);
        self.queue_driver = avail;
        self.queue_device = align_up(used_unaligned, u64::from(self.queue_align.max(1)));
    }

    fn config_read(&self, offset: u64, size: u8) -> u64 {
        let capacity = self.backend.capacity_sectors();
        let mut config = [0u8; 0x40];
        config[0..8].copy_from_slice(&capacity.to_le_bytes());
        config[0x14..0x18].copy_from_slice(&(SECTOR_SIZE as u32).to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    fn process_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        if !self.queue_ready || self.queue_num == 0 || self.queue_desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, self.queue_driver + 2) else {
            return;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut read_buf = std::mem::take(&mut self.read_scratch);
        while self.last_avail_idx != avail_idx {
            let ring_off = 4 + u64::from(self.last_avail_idx % self.queue_num) * 2;
            let Some(head) = read_u16(mem, self.queue_driver + ring_off) else {
                break;
            };
            let completion = self.process_descriptor_chain(mem, head, &mut descs, &mut read_buf);
            self.write_used(mem, head, completion.written_len);
            self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
            self.interrupt_status |= 1;
        }
        descs.clear();
        read_buf.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = read_buf;
    }

    fn process_descriptor_chain(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        head: u16,
        descs: &mut Vec<Descriptor>,
        read_buf: &mut Vec<u8>,
    ) -> RequestCompletion {
        if !Self::descriptor_chain_into(mem, self.queue_num, self.queue_desc, head, descs) {
            return RequestCompletion::status_only(mem, 0, VIRTIO_BLK_S_IOERR);
        }
        if descs.len() < 3 {
            return RequestCompletion::write_status(mem, descs.last(), VIRTIO_BLK_S_IOERR, 1);
        }
        let mut header = [0u8; 16];
        if !mem.read_into(descs[0].addr, &mut header) {
            return RequestCompletion::write_status(mem, descs.last(), VIRTIO_BLK_S_IOERR, 1);
        }
        let req_type = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let sector = u64::from_le_bytes([
            header[8], header[9], header[10], header[11], header[12], header[13], header[14],
            header[15],
        ]);
        let status = descs[descs.len() - 1];
        let data_descs = &descs[1..descs.len() - 1];
        let data_len_u64 = match data_descs
            .iter()
            .try_fold(0u64, |sum, desc| sum.checked_add(u64::from(desc.len)))
        {
            Some(len) => len,
            None => {
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
        };
        let data_len = u32::try_from(data_len_u64).unwrap_or(u32::MAX);
        self.stats.request_count = self.stats.request_count.saturating_add(1);
        self.stats.last_sector = Some(sector);

        if req_type != VIRTIO_BLK_T_IN {
            self.stats.unsupported_count = self.stats.unsupported_count.saturating_add(1);
            self.stats.last_status = Some(VIRTIO_BLK_S_UNSUPP);
            self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_UNSUPP);
            return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_UNSUPP, 1);
        }
        self.stats.read_count = self.stats.read_count.saturating_add(1);

        let mut byte_offset = match sector.checked_mul(SECTOR_SIZE) {
            Some(offset) => offset,
            None => {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
        };
        let media_len = self.backend.capacity_sectors().saturating_mul(SECTOR_SIZE);
        if byte_offset
            .checked_add(data_len_u64)
            .map_or(true, |end| end > media_len)
        {
            self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
            self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
            self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
            return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
        }
        let mut written_len = 0u32;
        for desc in data_descs {
            if desc.flags & DESC_F_WRITE == 0 {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
            let mut remaining = desc.len as usize;
            let mut guest_addr = desc.addr;
            while remaining > 0 {
                let chunk_len = remaining.min(READ_CHUNK_BYTES);
                read_buf.resize(chunk_len, 0);
                if self.backend.read_at_into(byte_offset, read_buf).is_err()
                    || !mem.write_bytes(guest_addr, read_buf.as_slice())
                {
                    self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                    self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                    self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                    return RequestCompletion::write_status(
                        mem,
                        Some(&status),
                        VIRTIO_BLK_S_IOERR,
                        1,
                    );
                }
                byte_offset = match byte_offset.checked_add(chunk_len as u64) {
                    Some(next) => next,
                    None => {
                        self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                        self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                        self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                        return RequestCompletion::write_status(
                            mem,
                            Some(&status),
                            VIRTIO_BLK_S_IOERR,
                            1,
                        );
                    }
                };
                guest_addr = match guest_addr.checked_add(chunk_len as u64) {
                    Some(next) => next,
                    None => {
                        self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                        self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                        self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                        return RequestCompletion::write_status(
                            mem,
                            Some(&status),
                            VIRTIO_BLK_S_IOERR,
                            1,
                        );
                    }
                };
                remaining -= chunk_len;
            }
            written_len = written_len.saturating_add(desc.len);
            self.stats.bytes_read = self.stats.bytes_read.saturating_add(u64::from(desc.len));
        }
        self.stats.last_len = written_len;
        self.stats.last_status = Some(VIRTIO_BLK_S_OK);
        self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_OK);
        RequestCompletion::write_status(
            mem,
            Some(&status),
            VIRTIO_BLK_S_OK,
            written_len.saturating_add(1),
        )
    }

    fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue_num: u16,
        queue_desc: u64,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        if head >= queue_num {
            return false;
        }
        let mut index = head;
        for _ in 0..queue_num {
            let Some(desc) = Descriptor::read(mem, queue_desc + u64::from(index) * DESC_SIZE)
            else {
                out.clear();
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue_num {
                out.clear();
                return false;
            }
        }
        out.clear();
        false
    }

    fn write_used(&self, mem: &mut dyn GuestMemoryMut, id: u16, len: u32) {
        let Some(used_idx) = read_u16(mem, self.queue_device + 2) else {
            return;
        };
        let elem = self.queue_device + 4 + u64::from(used_idx % self.queue_num) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(
            self.queue_device + 2,
            &used_idx.wrapping_add(1).to_le_bytes(),
        );
    }
}

#[derive(Debug)]
pub struct VirtioPciBlock {
    block: VirtioMmioBlock,
    msix: MsixTable,
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

    fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_BLK_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_BLK_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}

fn pci_to_mmio_offset(offset: u64, is_write: bool) -> Option<u64> {
    if (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(offset - PCI_COMMON_CFG_OFFSET);
    }
    if (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_CONFIG + offset - PCI_DEVICE_CFG_OFFSET);
    }
    if (PCI_NOTIFY_CFG_OFFSET..PCI_NOTIFY_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_QUEUE_NOTIFY);
    }
    if offset == PCI_ISR_CFG_OFFSET {
        return Some(if is_write {
            REG_INTERRUPT_ACK
        } else {
            REG_INTERRUPT_STATUS
        });
    }
    None
}

#[derive(Debug)]
struct RawFileBackend {
    file: File,
    len: u64,
}

impl RawFileBackend {
    fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }

    fn capacity_sectors(&self) -> u64 {
        self.len.div_ceil(SECTOR_SIZE)
    }

    fn read_at_into(&mut self, byte_offset: u64, dst: &mut [u8]) -> io::Result<()> {
        let end = byte_offset
            .checked_add(dst.len() as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "read offset overflow"))?;
        if end > self.capacity_sectors() * SECTOR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "virtio block read past media",
            ));
        }
        if byte_offset >= self.len {
            dst.fill(0);
            return Ok(());
        }
        let readable = (self.len - byte_offset).min(dst.len() as u64) as usize;
        self.file.seek(SeekFrom::Start(byte_offset))?;
        self.file.read_exact(&mut dst[..readable])?;
        if readable < dst.len() {
            dst[readable..].fill(0);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

impl Descriptor {
    fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
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

#[derive(Debug, Clone, Copy)]
struct RequestCompletion {
    written_len: u32,
}

impl RequestCompletion {
    fn status_only(mem: &mut dyn GuestMemoryMut, status_addr: u64, status: u8) -> Self {
        if status_addr != 0 {
            let _ = mem.write_bytes(status_addr, &[status]);
        }
        Self { written_len: 1 }
    }

    fn write_status(
        mem: &mut dyn GuestMemoryMut,
        status_desc: Option<&Descriptor>,
        status: u8,
        written_len: u32,
    ) -> Self {
        if let Some(desc) = status_desc {
            if desc.flags & DESC_F_WRITE != 0 && desc.len >= 1 {
                let _ = mem.write_bytes(desc.addr, &[status]);
            }
        }
        Self { written_len }
    }
}

fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    let align = align.max(1);
    value.div_ceil(align).saturating_mul(align)
}

fn read_le_from_bytes(bytes: &[u8], offset: u64, size: u8) -> Option<u64> {
    let offset = usize::try_from(offset).ok()?;
    let size = usize::from(size);
    if offset.checked_add(size)? > bytes.len() || size > 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&bytes[offset..offset + size]);
    Some(u64::from_le_bytes(buf))
}

fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests;

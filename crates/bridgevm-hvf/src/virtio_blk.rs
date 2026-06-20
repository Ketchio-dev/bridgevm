//! Minimal virtio-mmio block device for installer ISO media.
//!
//! QEMU's `virt` machine exposes 32 virtio-mmio transports. ArmVirtQemu's
//! firmware can boot a Windows ISO from a read-only virtio block device on one
//! of those transports and presents the El Torito image as `CDROM(0x0)`. This
//! module models just enough of QEMU's default legacy virtio-mmio transport and
//! split virtqueue block protocol for firmware reads from a host ISO file.

use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use crate::{fwcfg::GuestMemoryMut, machine};

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

const VIRTIO_F_VERSION_1: u32 = 1 << 0; // bit 32, selected through features_sel=1
const VIRTIO_BLK_F_RO: u32 = 1 << 5;
const VIRTIO_BLK_F_BLK_SIZE: u32 = 1 << 6;

const QUEUE_MAX: u16 = 128;
const SECTOR_SIZE: u64 = 512;
const DESC_SIZE: u64 = 16;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

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

    #[cfg(test)]
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
        })
    }

    pub fn len(&self) -> u64 {
        self.backend.len
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
        while self.last_avail_idx != avail_idx {
            let ring_off = 4 + u64::from(self.last_avail_idx % self.queue_num) * 2;
            let Some(head) = read_u16(mem, self.queue_driver + ring_off) else {
                return;
            };
            let completion = self.process_descriptor_chain(mem, head);
            self.write_used(mem, head, completion.written_len);
            self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
            self.interrupt_status |= 1;
        }
    }

    fn process_descriptor_chain(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        head: u16,
    ) -> RequestCompletion {
        let Some(descs) = self.descriptor_chain(mem, head) else {
            return RequestCompletion::status_only(mem, 0, VIRTIO_BLK_S_IOERR);
        };
        if descs.len() < 3 {
            return RequestCompletion::write_status(mem, descs.last(), VIRTIO_BLK_S_IOERR, 1);
        }
        let header = match mem.read_bytes(descs[0].addr, 16) {
            Some(bytes) if bytes.len() == 16 => bytes,
            _ => return RequestCompletion::write_status(mem, descs.last(), VIRTIO_BLK_S_IOERR, 1),
        };
        let req_type = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let sector = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let status = *descs.last().unwrap();
        let data_descs = &descs[1..descs.len() - 1];
        self.stats.request_count = self.stats.request_count.saturating_add(1);
        self.stats.last_sector = Some(sector);

        if req_type != VIRTIO_BLK_T_IN {
            self.stats.unsupported_count = self.stats.unsupported_count.saturating_add(1);
            self.stats.last_status = Some(VIRTIO_BLK_S_UNSUPP);
            return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_UNSUPP, 1);
        }
        self.stats.read_count = self.stats.read_count.saturating_add(1);

        let mut byte_offset = match sector.checked_mul(SECTOR_SIZE) {
            Some(offset) => offset,
            None => {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
        };
        let mut written_len = 0u32;
        for desc in data_descs {
            if desc.flags & DESC_F_WRITE == 0 {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
            let len = desc.len as usize;
            let Ok(data) = self.backend.read_at(byte_offset, len) else {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            };
            if !mem.write_bytes(desc.addr, &data) {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
            byte_offset = byte_offset.saturating_add(u64::from(desc.len));
            written_len = written_len.saturating_add(desc.len);
            self.stats.bytes_read = self.stats.bytes_read.saturating_add(u64::from(desc.len));
        }
        self.stats.last_len = written_len;
        self.stats.last_status = Some(VIRTIO_BLK_S_OK);
        RequestCompletion::write_status(
            mem,
            Some(&status),
            VIRTIO_BLK_S_OK,
            written_len.saturating_add(1),
        )
    }

    fn descriptor_chain(&self, mem: &dyn GuestMemoryMut, head: u16) -> Option<Vec<Descriptor>> {
        let mut out = Vec::new();
        let mut index = head;
        for _ in 0..self.queue_num {
            let desc = Descriptor::read(mem, self.queue_desc + u64::from(index) * DESC_SIZE)?;
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return Some(out);
            }
            index = desc.next;
            if index >= self.queue_num {
                return None;
            }
        }
        None
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

    fn read_at(&mut self, byte_offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let end = byte_offset
            .checked_add(len as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "read offset overflow"))?;
        if end > self.capacity_sectors() * SECTOR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "virtio block read past media",
            ));
        }
        let mut data = vec![0u8; len];
        if byte_offset < self.len {
            let readable = (self.len - byte_offset).min(len as u64) as usize;
            self.file.seek(SeekFrom::Start(byte_offset))?;
            self.file.read_exact(&mut data[..readable])?;
        }
        Ok(data)
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
        let bytes = mem.read_bytes(gpa, DESC_SIZE as usize)?;
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().ok()?),
            len: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            flags: u16::from_le_bytes(bytes[12..14].try_into().ok()?),
            next: u16::from_le_bytes(bytes[14..16].try_into().ok()?),
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
    let bytes = mem.read_bytes(gpa, 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, time::SystemTime};

    #[derive(Debug)]
    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl TestMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0; len],
            }
        }

        fn write(&mut self, gpa: u64, data: &[u8]) {
            assert!(self.write_bytes(gpa, data));
        }

        fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
            self.read_bytes(gpa, len).unwrap()
        }
    }

    impl GuestMemoryMut for TestMem {
        fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
            let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
                return false;
            };
            if off + data.len() > self.bytes.len() {
                return false;
            }
            self.bytes[off..off + data.len()].copy_from_slice(data);
            true
        }

        fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
            let off = gpa.checked_sub(self.base)? as usize;
            (off + len <= self.bytes.len()).then(|| self.bytes[off..off + len].to_vec())
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bridgevm-hvf-virtio-blk-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_desc(mem: &mut TestMem, table: u64, index: u16, desc: Descriptor) {
        let gpa = table + u64::from(index) * DESC_SIZE;
        mem.write(gpa, &desc.addr.to_le_bytes());
        mem.write(gpa + 8, &desc.len.to_le_bytes());
        mem.write(gpa + 12, &desc.flags.to_le_bytes());
        mem.write(gpa + 14, &desc.next.to_le_bytes());
    }

    #[test]
    fn identity_and_capacity_registers_are_exposed() {
        let path = temp_path("identity");
        fs::write(&path, vec![0u8; 1500]).unwrap();
        let mut dev = VirtioMmioBlock::open_read_only(&path).unwrap();
        let mut mem = TestMem::new(0x4000_0000, 0x1000);

        assert_eq!(
            dev.access(REG_MAGIC, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(u64::from(MAGIC_VALUE))
        );
        assert_eq!(
            dev.access(REG_VERSION, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(u64::from(VERSION_LEGACY))
        );
        assert_eq!(
            dev.access(REG_DEVICE_ID, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(u64::from(DEVICE_ID_BLOCK))
        );
        assert_eq!(
            dev.access(REG_DEVICE_FEATURES, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE))
        );
        assert_eq!(
            dev.access(REG_CONFIG, false, 8, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(3)
        );
        assert_eq!(
            dev.access(REG_CONFIG + 0x14, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(SECTOR_SIZE)
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn legacy_read_request_copies_media_to_guest_and_posts_used_element() {
        let path = temp_path("read");
        let mut media = vec![0u8; 1024];
        media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&path, media).unwrap();
        let mut dev = VirtioMmioBlock::open_read_only(&path).unwrap();
        let mut mem = TestMem::new(0x4000_0000, 0x10000);

        let desc = 0x4000_1000;
        let avail = desc + 8 * DESC_SIZE;
        let used = align_up(avail + 4 + 8 * 2, 4096);
        let header = 0x4000_4000;
        let data = 0x4000_5000;
        let status = 0x4000_6000;

        mem.write(header, &VIRTIO_BLK_T_IN.to_le_bytes());
        mem.write(header + 8, &1u64.to_le_bytes());
        write_desc(
            &mut mem,
            desc,
            0,
            Descriptor {
                addr: header,
                len: 16,
                flags: DESC_F_NEXT,
                next: 1,
            },
        );
        write_desc(
            &mut mem,
            desc,
            1,
            Descriptor {
                addr: data,
                len: 512,
                flags: DESC_F_NEXT | DESC_F_WRITE,
                next: 2,
            },
        );
        write_desc(
            &mut mem,
            desc,
            2,
            Descriptor {
                addr: status,
                len: 1,
                flags: DESC_F_WRITE,
                next: 0,
            },
        );
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());

        dev.access(REG_QUEUE_NUM, true, 4, 8, &mut mem);
        dev.access(REG_GUEST_PAGE_SIZE, true, 4, 4096, &mut mem);
        dev.access(REG_QUEUE_ALIGN, true, 4, 4096, &mut mem);
        dev.access(REG_QUEUE_PFN, true, 4, desc >> 12, &mut mem);
        dev.access(REG_QUEUE_NOTIFY, true, 4, 0, &mut mem);

        assert_eq!(&mem.read(data, 8), b"WINSETUP");
        assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            1
        );
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
            513
        );
        assert_eq!(
            dev.access(REG_INTERRUPT_STATUS, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(1)
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn modern_read_request_copies_media_to_guest_and_posts_used_element() {
        let path = temp_path("modern-read");
        let mut media = vec![0u8; 1024];
        media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&path, media).unwrap();
        let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
        let mut mem = TestMem::new(0x4000_0000, 0x10000);

        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let header = 0x4000_4000;
        let data = 0x4000_5000;
        let status = 0x4000_6000;

        mem.write(header, &VIRTIO_BLK_T_IN.to_le_bytes());
        mem.write(header + 8, &1u64.to_le_bytes());
        write_desc(
            &mut mem,
            desc,
            0,
            Descriptor {
                addr: header,
                len: 16,
                flags: DESC_F_NEXT,
                next: 1,
            },
        );
        write_desc(
            &mut mem,
            desc,
            1,
            Descriptor {
                addr: data,
                len: 512,
                flags: DESC_F_NEXT | DESC_F_WRITE,
                next: 2,
            },
        );
        write_desc(
            &mut mem,
            desc,
            2,
            Descriptor {
                addr: status,
                len: 1,
                flags: DESC_F_WRITE,
                next: 0,
            },
        );
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());

        dev.access(REG_QUEUE_NUM, true, 4, 8, &mut mem);
        dev.access(REG_QUEUE_DESC_LOW, true, 4, desc, &mut mem);
        dev.access(REG_QUEUE_DRIVER_LOW, true, 4, avail, &mut mem);
        dev.access(REG_QUEUE_DEVICE_LOW, true, 4, used, &mut mem);
        dev.access(REG_QUEUE_READY, true, 4, 1, &mut mem);
        dev.access(REG_QUEUE_NOTIFY, true, 4, 0, &mut mem);

        assert_eq!(&mem.read(data, 8), b"WINSETUP");
        assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            1
        );
        assert_eq!(
            dev.access(REG_INTERRUPT_STATUS, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(1)
        );

        fs::remove_file(path).ok();
    }
}

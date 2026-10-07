//! Legacy `fw_cfg` firmware-compatibility adapter.
//!
//! BridgeVM's pinned ArmVirtQemu EDK2 code volume discovers guest ACPI tables,
//! SMBIOS, boot order and direct-boot payloads through the published `fw_cfg`
//! wire ABI. This module isolates that firmware dependency from BridgeVM's
//! independently implemented platform and device models.
//!
//! The compatibility window lives at MMIO base `0x0902_0000`, has size `0x18`,
//! and retains the protocol's `qemu,fw-cfg-mmio` DT identifier. This module
//! models only the selector/data and DMA register ABI. The HVF run loop maps guest
//! MMIO accesses onto [`FwCfg::mmio_read`] / [`FwCfg::mmio_write`] and supplies a
//! [`GuestMemoryMut`] accessor so the DMA path can move bytes in and out of guest
//! RAM. The literal signatures and selector values below are interoperability
//! identifiers, not implementation-source provenance.

use std::collections::BTreeMap;

/// MMIO base of the firmware-compatibility window (`fw-cfg@9020000`).
pub const FW_CFG_MMIO_BASE: u64 = 0x0902_0000;
/// MMIO window size (`reg = <... 0x18>`): DATA(8) + SELECTOR(2)+pad + DMA(8).
pub const FW_CFG_MMIO_SIZE: u64 = 0x18;

// Register offsets within the MMIO window.
const REG_DATA: u64 = 0x00; // 0x00..0x08, byte stream of the selected entry
const REG_SELECTOR: u64 = 0x08; // 0x08..0x0A, 16-bit, big-endian
const REG_DMA: u64 = 0x10; // 0x10..0x18, 64-bit, big-endian

// Standard selector keys.
/// `FW_CFG_SIGNATURE` — reads back the ASCII bytes `"QEMU"`.
pub const KEY_SIGNATURE: u16 = 0x0000;
/// `FW_CFG_ID` — reads back a little-endian `u32` feature bitmap.
pub const KEY_ID: u16 = 0x0001;
/// `FW_CFG_FILE_DIR` — reads back the named-file directory.
pub const KEY_FILE_DIR: u16 = 0x0019;
/// First selector handed out to dynamically registered named files.
pub const KEY_FILE_FIRST: u16 = 0x0020;

/// `FW_CFG_KERNEL_SIZE` — direct-boot kernel payload size.
pub const KEY_KERNEL_SIZE: u16 = 0x0008;
/// `FW_CFG_INITRD_SIZE` — direct-boot initrd payload size.
pub const KEY_INITRD_SIZE: u16 = 0x000b;
/// `FW_CFG_KERNEL_DATA` — direct-boot kernel payload bytes.
pub const KEY_KERNEL_DATA: u16 = 0x0011;
/// `FW_CFG_INITRD_DATA` — direct-boot initrd payload bytes.
pub const KEY_INITRD_DATA: u16 = 0x0012;
/// `FW_CFG_CMDLINE_SIZE` — direct-boot command-line size.
pub const KEY_CMDLINE_SIZE: u16 = 0x0014;
/// `FW_CFG_CMDLINE_DATA` — direct-boot command-line bytes.
pub const KEY_CMDLINE_DATA: u16 = 0x0015;

// `FW_CFG_ID` feature bits.
const ID_TRADITIONAL: u32 = 0x01;
const ID_DMA: u32 = 0x02;

// DMA control-word bits (big-endian on the wire).
/// Set by the device in the returned control word on failure.
pub const DMA_CTL_ERROR: u32 = 0x01;
/// Transfer from `fw_cfg` into guest memory.
pub const DMA_CTL_READ: u32 = 0x02;
/// Advance the read cursor without transferring.
pub const DMA_CTL_SKIP: u32 = 0x04;
/// Select the entry named in the upper 16 bits of the control word.
pub const DMA_CTL_SELECT: u32 = 0x08;
/// Transfer from guest memory into `fw_cfg` (writable entries only).
pub const DMA_CTL_WRITE: u32 = 0x10;

/// The 8-byte big-endian signature returned by reading the DMA register; the
/// firmware reads it to confirm the DMA interface is present (`"QEMU CFG"`).
pub const DMA_REG_SIGNATURE: u64 = 0x5145_4d55_2043_4647;

/// Accessor the DMA path uses to move bytes in and out of guest RAM.
pub trait GuestMemoryMut {
    /// Write `data` starting at guest-physical address `gpa`. Returns `false`
    /// if the range is not backed (the DMA then reports `DMA_CTL_ERROR`).
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool;
    /// Read `len` bytes starting at guest-physical address `gpa`, or `None` if
    /// the range is not backed.
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>>;
    /// Read `dst.len()` bytes starting at guest-physical address `gpa` into
    /// `dst`. Returns `false` if the range is not backed. Unlike [`read_bytes`]
    /// this fills a caller-owned buffer, so hot paths (NVMe SQE/PRP-list fetch)
    /// avoid a per-read heap allocation. The default routes through `read_bytes`
    /// for correctness; live guest-RAM views override it to copy straight out of
    /// the mapping.
    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        match self.read_bytes(gpa, dst.len()) {
            Some(bytes) => {
                dst.copy_from_slice(&bytes);
                true
            }
            None => false,
        }
    }
    /// Resolve a guest-physical span to its stable host pointer. Device models
    /// use this only when a backend must retain guest RAM iovecs for a resource
    /// lifetime; live callers point at the fixed HVF guest RAM mapping.
    fn host_ptr(&self, _gpa: u64, _len: usize) -> Option<*mut u8> {
        None
    }
}

/// A decoded `FWCfgDmaAccess` control structure (all fields big-endian on the
/// wire; this struct holds host-order values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FwCfgDmaAccess {
    pub control: u32,
    pub length: u32,
    pub address: u64,
}

impl FwCfgDmaAccess {
    /// Decode the 16-byte big-endian control structure as read from guest RAM.
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        let control = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let length = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let address = u64::from_be_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        Self {
            control,
            length,
            address,
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    data: Vec<u8>,
    /// Writable entries accept `DMA_CTL_WRITE`; static metadata does not.
    writable: bool,
}

#[derive(Debug, Clone)]
struct FileMeta {
    name: String,
    select: u16,
    size: u32,
}

/// The modelled `fw_cfg` compatibility boundary.
#[derive(Debug, Clone)]
pub struct FwCfg {
    entries: BTreeMap<u16, Entry>,
    files: Vec<FileMeta>,
    selector: u16,
    offset: usize,
    next_file_selector: u16,
}

impl Default for FwCfg {
    fn default() -> Self {
        Self::new()
    }
}

impl FwCfg {
    /// Create a device pre-populated with the mandatory `SIGNATURE`, `ID` and an
    /// empty `FILE_DIR` entry required by the declared reset contract.
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(
            KEY_SIGNATURE,
            Entry {
                data: b"QEMU".to_vec(),
                writable: false,
            },
        );
        entries.insert(
            KEY_ID,
            Entry {
                data: (ID_TRADITIONAL | ID_DMA).to_le_bytes().to_vec(),
                writable: false,
            },
        );
        let mut fw = Self {
            entries,
            files: Vec::new(),
            selector: KEY_SIGNATURE,
            offset: 0,
            next_file_selector: KEY_FILE_FIRST,
            // FILE_DIR is (re)generated by `rebuild_file_dir`.
        };
        fw.rebuild_file_dir();
        fw
    }

    /// Register a named blob (e.g. `"etc/acpi/tables"`). Returns the selector
    /// assigned to it. Names are surfaced in `FILE_DIR` sorted lexically, which
    /// is the order the firmware expects.
    pub fn add_file(&mut self, name: &str, data: Vec<u8>) -> u16 {
        self.add_entry(name, data, false)
    }

    /// Register a writable named blob — the firmware may push bytes back into it
    /// via `DMA_CTL_WRITE` (used for `"etc/system-states"` and similar).
    pub fn add_writable_file(&mut self, name: &str, data: Vec<u8>) -> u16 {
        self.add_entry(name, data, true)
    }

    /// Register a fixed selector item that does not appear in `FILE_DIR`.
    ///
    /// ArmVirtQemu's `QemuKernelLoaderFsDxe` consumes QEMU direct-kernel-boot
    /// payloads through traditional fw_cfg keys (`KERNEL_SIZE`, `KERNEL_DATA`,
    /// `INITRD_*`, `CMDLINE_*`) rather than named files.
    pub fn add_item(&mut self, key: u16, data: Vec<u8>) {
        assert!(
            key < KEY_FILE_FIRST && key != KEY_FILE_DIR,
            "fixed fw_cfg item key must be below KEY_FILE_FIRST and not FILE_DIR: {key:#x}"
        );
        self.entries.insert(
            key,
            Entry {
                data,
                writable: false,
            },
        );
    }

    fn add_entry(&mut self, name: &str, data: Vec<u8>, writable: bool) -> u16 {
        assert!(
            name.len() < 56,
            "fw_cfg file name must be < 56 bytes: {name:?}"
        );
        if let Some(file) = self.files.iter_mut().find(|file| file.name == name) {
            let select = file.select;
            // SAFE-EXPECT: fw_cfg blobs are host-constructed and the directory format is u32-sized.
            file.size = u32::try_from(data.len()).expect("fw_cfg file exceeds 4 GiB");
            self.entries.insert(select, Entry { data, writable });
            self.rebuild_file_dir();
            return select;
        }
        let select = self.next_file_selector;
        self.next_file_selector = self
            .next_file_selector
            .checked_add(1)
            // SAFE-EXPECT: selector exhaustion requires constructing >64K fw_cfg files.
            .expect("fw_cfg selector space exhausted");
        // SAFE-EXPECT: fw_cfg blobs are host-constructed and the directory format is u32-sized.
        let size = u32::try_from(data.len()).expect("fw_cfg file exceeds 4 GiB");
        self.entries.insert(select, Entry { data, writable });
        self.files.push(FileMeta {
            name: name.to_string(),
            select,
            size,
        });
        self.rebuild_file_dir();
        select
    }

    /// Regenerate the `FILE_DIR` blob: big-endian `u32` count followed by one
    /// 64-byte `FWCfgFile` record per file (`size:u32`, `select:u16`,
    /// `reserved:u16`, `name[56]`), sorted by name.
    fn rebuild_file_dir(&mut self) {
        let mut sorted: Vec<&FileMeta> = self.files.iter().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));

        let mut blob = Vec::with_capacity(4 + sorted.len() * 64);
        blob.extend_from_slice(&(sorted.len() as u32).to_be_bytes());
        for file in sorted {
            blob.extend_from_slice(&file.size.to_be_bytes());
            blob.extend_from_slice(&file.select.to_be_bytes());
            blob.extend_from_slice(&0u16.to_be_bytes()); // reserved
            let mut name = [0u8; 56];
            let bytes = file.name.as_bytes();
            name[..bytes.len()].copy_from_slice(bytes);
            blob.extend_from_slice(&name);
        }
        self.entries.insert(
            KEY_FILE_DIR,
            Entry {
                data: blob,
                writable: false,
            },
        );
    }

    /// Select an entry and reset its read cursor (the selector register write).
    pub fn select(&mut self, key: u16) {
        self.selector = key;
        self.offset = 0;
    }

    /// The currently selected entry's bytes, if any.
    fn current(&self) -> Option<&Entry> {
        self.entries.get(&self.selector)
    }

    /// Read the next byte of the selected entry, advancing the cursor. Reads past
    /// the end (or of an unknown selector) return `0` by protocol policy.
    pub fn read_data_byte(&mut self) -> u8 {
        let byte = self
            .current()
            .and_then(|e| e.data.get(self.offset).copied())
            .unwrap_or(0);
        self.offset = self.offset.saturating_add(1);
        byte
    }

    /// Read `n` bytes of the selected entry as a stream.
    pub fn read_data(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.read_data_byte()).collect()
    }

    pub fn reset_runtime_state(&mut self) {
        self.selector = KEY_SIGNATURE;
        self.offset = 0;
    }

    pub fn reset_file_bytes(&mut self, name: &str, fill: u8) -> bool {
        let Some(select) = self
            .files
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.select)
        else {
            self.reset_runtime_state();
            return false;
        };
        let Some(entry) = self.entries.get_mut(&select) else {
            self.reset_runtime_state();
            return false;
        };
        if !entry.writable {
            self.reset_runtime_state();
            return false;
        }
        entry.data.fill(fill);
        self.reset_runtime_state();
        true
    }

    /// The raw `FILE_DIR` blob, for callers that want to inspect it directly.
    pub fn file_dir_bytes(&self) -> &[u8] {
        &self.entries[&KEY_FILE_DIR].data
    }

    pub fn file_bytes(&self, name: &str) -> Option<&[u8]> {
        let select = self
            .files
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.select)?;
        self.entries.get(&select).map(|entry| entry.data.as_slice())
    }

    // ---- MMIO register interface -------------------------------------------
    //
    // The `qemu,fw-cfg-mmio` selector and DMA registers are big-endian. DATA is
    // a byte stream consumed by normal little-endian AArch64 loads: a 32-bit
    // The protocol literal "QEMU" is exposed in little-endian register order.
    // (0x554d4551), while big-endian entries such as FILE_DIR remain
    // big-endian bytes that firmware explicitly swaps after reading.

    /// Handle a guest MMIO read of `size` bytes at `offset` within the window.
    pub fn mmio_read(&mut self, offset: u64, size: u8) -> u64 {
        match offset {
            REG_DATA => {
                let mut value: u64 = 0;
                for shift in 0..size {
                    value |= u64::from(self.read_data_byte()) << (u64::from(shift) * 8);
                }
                value
            }
            REG_DMA => DMA_REG_SIGNATURE,
            _ => 0,
        }
    }

    /// Handle a guest MMIO write of `size` bytes at `offset` within the window.
    /// `value` is the raw value the guest stored (native byte order). The selector
    /// and DMA registers are **big-endian** per `qemu,fw-cfg-mmio` — guest firmware
    /// stores `SwapBytes16(selector)` / `SwapBytes64(dma_addr)` — so swap to recover
    /// the logical value. A write to the DMA register triggers a transfer via `mem`.
    pub fn mmio_write(&mut self, offset: u64, _size: u8, value: u64, mem: &mut dyn GuestMemoryMut) {
        match offset {
            REG_SELECTOR => self.select((value as u16).swap_bytes()),
            REG_DMA => self.run_dma_at(value.swap_bytes(), mem),
            _ => {}
        }
    }

    /// Read a `FWCfgDmaAccess` structure from guest RAM at `ctrl_gpa`, run it, and
    /// write the resulting control word back (big-endian) at the same address.
    fn run_dma_at(&mut self, ctrl_gpa: u64, mem: &mut dyn GuestMemoryMut) {
        let mut buf = [0u8; 16];
        if !mem.read_into(ctrl_gpa, &mut buf) {
            return;
        }
        let access = FwCfgDmaAccess::from_bytes(&buf);
        let result = self.dma_execute(access, mem);
        let _ = mem.write_bytes(ctrl_gpa, &result.to_be_bytes());
    }

    /// Execute a decoded DMA access. Returns the control word to report back:
    /// `0` on success, `DMA_CTL_ERROR` on failure (per the spec the device
    /// clears every other bit when it finishes).
    pub fn dma_execute(&mut self, access: FwCfgDmaAccess, mem: &mut dyn GuestMemoryMut) -> u32 {
        let mut control = access.control;

        if control & DMA_CTL_SELECT != 0 {
            self.select((control >> 16) as u16);
        }
        let length = access.length as usize;

        if control & DMA_CTL_READ != 0 {
            if !self.dma_read_into_guest(access.address, length, mem) {
                return DMA_CTL_ERROR;
            }
        } else if control & DMA_CTL_WRITE != 0 {
            // Writable entries only; bytes flow guest -> fw_cfg.
            let writable = self.current().map(|e| e.writable).unwrap_or(false);
            if !writable {
                return DMA_CTL_ERROR;
            }
            if !guest_range_readable(mem, access.address, length) {
                return DMA_CTL_ERROR;
            }
            if let Some(entry) = self.entries.get_mut(&self.selector) {
                if !copy_guest_prefix_into_entry(mem, access.address, length, self.offset, entry) {
                    return DMA_CTL_ERROR;
                }
            }
            self.offset = self.offset.saturating_add(length);
        } else if control & DMA_CTL_SKIP != 0 {
            self.offset = self.offset.saturating_add(length);
        }

        control = 0; // success: device clears all bits
        control
    }

    /// Stream the selected entry into guest RAM in bounded chunks, zero-filling
    /// past its end. The guest-chosen length (up to 4 GiB) is never staged in a
    /// host buffer, and the first unbacked chunk stops the transfer.
    fn dma_read_into_guest(
        &mut self,
        mut gpa: u64,
        mut len: usize,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        const ZERO_FILL: [u8; 4096] = [0; 4096];
        while len != 0 {
            let available = self
                .current()
                .and_then(|entry| entry.data.get(self.offset..))
                .unwrap_or(&[]);
            let (chunk, written) = if available.is_empty() {
                let chunk = len.min(ZERO_FILL.len());
                (chunk, mem.write_bytes(gpa, &ZERO_FILL[..chunk]))
            } else {
                let chunk = len.min(available.len());
                (chunk, mem.write_bytes(gpa, &available[..chunk]))
            };
            self.offset = self.offset.saturating_add(chunk);
            len -= chunk;
            match gpa.checked_add(chunk as u64) {
                Some(next) if written => gpa = next,
                _ => return written && len == 0,
            }
        }
        true
    }
}

fn guest_range_readable(mem: &dyn GuestMemoryMut, mut gpa: u64, mut len: usize) -> bool {
    let mut scratch = [0u8; 256];
    while len != 0 {
        let chunk = len.min(scratch.len());
        if !mem.read_into(gpa, &mut scratch[..chunk]) {
            return false;
        }
        let Some(next_gpa) = gpa.checked_add(chunk as u64) else {
            return false;
        };
        gpa = next_gpa;
        len -= chunk;
    }
    true
}

fn copy_guest_prefix_into_entry(
    mem: &dyn GuestMemoryMut,
    mut gpa: u64,
    mut remaining: usize,
    entry_offset: usize,
    entry: &mut Entry,
) -> bool {
    let mut consumed = 0usize;
    let mut scratch = [0u8; 256];
    while remaining != 0 {
        let Some(pos) = entry_offset.checked_add(consumed) else {
            return true;
        };
        if pos >= entry.data.len() {
            return true;
        }
        let chunk = remaining
            .min(scratch.len())
            .min(entry.data.len().saturating_sub(pos));
        if !mem.read_into(gpa, &mut scratch[..chunk]) {
            return false;
        }
        entry.data[pos..pos + chunk].copy_from_slice(&scratch[..chunk]);
        let Some(next_gpa) = gpa.checked_add(chunk as u64) else {
            return false;
        };
        gpa = next_gpa;
        consumed += chunk;
        remaining -= chunk;
    }
    true
}

#[cfg(test)]
#[path = "fwcfg_tests.rs"]
mod tests;

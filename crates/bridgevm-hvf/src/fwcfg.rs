//! QEMU `fw_cfg` (firmware configuration) MMIO device model.
//!
//! This is the keystone device for the BridgeVM HVF "QEMU virt contract" path
//! (Path A in `docs/hvf-windows-engine-strategy.md`). Stock ArmVirtQemu firmware
//! (`edk2-aarch64-code.fd`, the file the live HVF smokes already load) discovers
//! the guest ACPI tables, SMBIOS, boot order and the kernel/initrd through
//! `fw_cfg`. Without it the firmware has no ACPI to hand to a Windows or Linux
//! guest — the root cause catalogued in
//! `docs/hvf-windows-platform-contract-gap.md`.
//!
//! On the QEMU `virt` machine the device lives at MMIO base `0x0902_0000`,
//! window size `0x18`, device-tree `compatible = "qemu,fw-cfg-mmio"`. This module
//! models that register file: the selector/data "traditional" interface and the
//! DMA interface. It is host-only and unit-testable; the HVF run loop maps guest
//! MMIO accesses onto [`FwCfg::mmio_read`] / [`FwCfg::mmio_write`] and supplies a
//! [`GuestMemoryMut`] accessor so the DMA path can move bytes in and out of guest
//! RAM. Nothing here calls into Hypervisor.framework, so it builds and tests on
//! any host.
//!
//! References: QEMU `hw/nvram/fw_cfg.c`, `docs/specs/fw_cfg.txt`, and the
//! `qemu,fw-cfg-mmio` device-tree binding.

use std::collections::BTreeMap;

/// MMIO base of `fw_cfg` on the QEMU `virt` machine (`fw-cfg@9020000`).
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

/// `FW_CFG_KERNEL_SIZE` — QEMU `-kernel` payload size.
pub const KEY_KERNEL_SIZE: u16 = 0x0008;
/// `FW_CFG_INITRD_SIZE` — QEMU `-initrd` payload size.
pub const KEY_INITRD_SIZE: u16 = 0x000b;
/// `FW_CFG_KERNEL_DATA` — QEMU `-kernel` payload bytes.
pub const KEY_KERNEL_DATA: u16 = 0x0011;
/// `FW_CFG_INITRD_DATA` — QEMU `-initrd` payload bytes.
pub const KEY_INITRD_DATA: u16 = 0x0012;
/// `FW_CFG_CMDLINE_SIZE` — QEMU `-append` command line size.
pub const KEY_CMDLINE_SIZE: u16 = 0x0014;
/// `FW_CFG_CMDLINE_DATA` — QEMU `-append` command line bytes.
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

/// A modelled QEMU `fw_cfg` device.
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
    /// (empty) `FILE_DIR` entry, matching a freshly reset QEMU `fw_cfg`.
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
    /// the end (or of an unknown selector) return `0`, matching QEMU.
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
    // read of bytes "QEMU" must produce SIGNATURE_32('Q','E','M','U')
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
        let Some(raw) = mem.read_bytes(ctrl_gpa, 16) else {
            return;
        };
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&raw);
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
            let chunk = self.read_data(length);
            if !mem.write_bytes(access.address, &chunk) {
                return DMA_CTL_ERROR;
            }
        } else if control & DMA_CTL_WRITE != 0 {
            // Writable entries only; bytes flow guest -> fw_cfg.
            let writable = self.current().map(|e| e.writable).unwrap_or(false);
            if !writable {
                return DMA_CTL_ERROR;
            }
            let Some(src) = mem.read_bytes(access.address, length) else {
                return DMA_CTL_ERROR;
            };
            if let Some(entry) = self.entries.get_mut(&self.selector) {
                for (i, byte) in src.into_iter().enumerate() {
                    let pos = self.offset + i;
                    if pos < entry.data.len() {
                        entry.data[pos] = byte;
                    }
                }
            }
            self.offset = self.offset.saturating_add(length);
        } else if control & DMA_CTL_SKIP != 0 {
            self.offset = self.offset.saturating_add(length);
        }

        control = 0; // success: device clears all bits
        control
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat span of guest RAM for exercising the DMA path in tests.
    struct FakeMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl FakeMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0u8; len],
            }
        }
        fn at(&self, gpa: u64) -> usize {
            (gpa - self.base) as usize
        }
    }

    impl GuestMemoryMut for FakeMem {
        fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
            let start = self.at(gpa);
            let end = start + data.len();
            if end > self.bytes.len() {
                return false;
            }
            self.bytes[start..end].copy_from_slice(data);
            true
        }
        fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
            let start = self.at(gpa);
            let end = start + len;
            if end > self.bytes.len() {
                return None;
            }
            Some(self.bytes[start..end].to_vec())
        }
    }

    #[test]
    fn signature_reads_qemu() {
        let mut fw = FwCfg::new();
        fw.select(KEY_SIGNATURE);
        assert_eq!(fw.read_data(4), b"QEMU");
    }

    #[test]
    fn id_advertises_traditional_and_dma() {
        let mut fw = FwCfg::new();
        fw.select(KEY_ID);
        let bytes = fw.read_data(4);
        let id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(id & ID_TRADITIONAL, ID_TRADITIONAL);
        assert_eq!(id & ID_DMA, ID_DMA);
    }

    #[test]
    fn reading_past_end_returns_zero() {
        let mut fw = FwCfg::new();
        fw.select(KEY_SIGNATURE); // 4 bytes
        assert_eq!(fw.read_data(4), b"QEMU");
        assert_eq!(fw.read_data_byte(), 0);
    }

    #[test]
    fn reselect_resets_cursor() {
        let mut fw = FwCfg::new();
        fw.select(KEY_SIGNATURE);
        assert_eq!(fw.read_data_byte(), b'Q');
        fw.select(KEY_SIGNATURE);
        assert_eq!(fw.read_data_byte(), b'Q');
    }

    #[test]
    fn file_registration_assigns_sequential_selectors() {
        let mut fw = FwCfg::new();
        let a = fw.add_file("etc/acpi/rsdp", vec![1, 2, 3]);
        let b = fw.add_file("etc/acpi/tables", vec![4, 5]);
        assert_eq!(a, KEY_FILE_FIRST);
        assert_eq!(b, KEY_FILE_FIRST + 1);
        // The blob is reachable through its selector.
        fw.select(b);
        assert_eq!(fw.read_data(2), vec![4, 5]);
    }

    #[test]
    fn registering_the_same_file_replaces_without_duplicate_directory_entry() {
        let mut fw = FwCfg::new();
        let first = fw.add_file("etc/acpi/tables", vec![1, 2]);
        let second = fw.add_file("etc/acpi/tables", vec![3, 4, 5]);
        assert_eq!(first, second);

        fw.select(first);
        assert_eq!(fw.read_data(3), vec![3, 4, 5]);

        fw.select(KEY_FILE_DIR);
        let dir = fw.read_data(fw.file_dir_bytes().len());
        let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]);
        assert_eq!(count, 1);
        let size = u32::from_be_bytes([dir[4], dir[5], dir[6], dir[7]]);
        assert_eq!(size, 3);
    }

    #[test]
    fn fixed_items_are_readable_without_file_dir_entries() {
        let mut fw = FwCfg::new();
        fw.add_item(KEY_KERNEL_SIZE, 4u32.to_le_bytes().to_vec());
        fw.add_item(KEY_KERNEL_DATA, b"boot".to_vec());

        fw.select(KEY_KERNEL_SIZE);
        assert_eq!(fw.mmio_read(REG_DATA, 4), 4);
        fw.select(KEY_KERNEL_DATA);
        assert_eq!(fw.read_data(4), b"boot");

        fw.select(KEY_FILE_DIR);
        let dir = fw.read_data(fw.file_dir_bytes().len());
        let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]);
        assert_eq!(count, 0, "fixed selector items stay out of FILE_DIR");
    }

    #[test]
    fn directory_is_sorted_by_name_with_be_fields() {
        let mut fw = FwCfg::new();
        // Insert out of lexical order; directory must come back sorted.
        let tables = fw.add_file("etc/table-loader", vec![0; 7]);
        let rsdp = fw.add_file("etc/acpi/rsdp", vec![0; 36]);

        fw.select(KEY_FILE_DIR);
        let dir = fw.read_data(fw.file_dir_bytes().len());

        let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]);
        assert_eq!(count, 2);

        // First record (offset 4) must be the lexically-smallest name.
        let rec0 = &dir[4..68];
        let size0 = u32::from_be_bytes([rec0[0], rec0[1], rec0[2], rec0[3]]);
        let select0 = u16::from_be_bytes([rec0[4], rec0[5]]);
        let name0_end = rec0[8..64].iter().position(|&b| b == 0).unwrap_or(56);
        let name0 = std::str::from_utf8(&rec0[8..8 + name0_end]).unwrap();
        assert_eq!(name0, "etc/acpi/rsdp");
        assert_eq!(size0, 36);
        assert_eq!(select0, rsdp);

        let rec1 = &dir[68..132];
        let select1 = u16::from_be_bytes([rec1[4], rec1[5]]);
        let name1_end = rec1[8..64].iter().position(|&b| b == 0).unwrap_or(56);
        let name1 = std::str::from_utf8(&rec1[8..8 + name1_end]).unwrap();
        assert_eq!(name1, "etc/table-loader");
        assert_eq!(select1, tables);
    }

    #[test]
    fn mmio_data_read_is_little_endian_cpu_load_from_stream() {
        let mut fw = FwCfg::new();
        fw.select(KEY_SIGNATURE);
        // AArch64 firmware does `MmioRead32(DATA)` and compares against
        // SIGNATURE_32('Q','E','M','U') == 0x554d4551.
        assert_eq!(fw.mmio_read(REG_DATA, 4), 0x554d_4551);
    }

    #[test]
    fn mmio_dma_register_reads_signature() {
        let mut fw = FwCfg::new();
        assert_eq!(fw.mmio_read(REG_DMA, 8), DMA_REG_SIGNATURE);
    }

    #[test]
    fn mmio_selector_write_then_read() {
        let mut fw = FwCfg::new();
        let mut mem = FakeMem::new(0x4000_0000, 0);
        fw.mmio_write(REG_SELECTOR, 2, u64::from(KEY_SIGNATURE), &mut mem);
        assert_eq!(fw.mmio_read(REG_DATA, 1), u64::from(b'Q'));
    }

    #[test]
    fn mmio_selector_register_is_big_endian() {
        // Guest firmware stores SwapBytes16(selector); selecting FILE_DIR (0x0019)
        // arrives on the bus as 0x1900. The device must swap it back, not read a
        // non-existent item.
        let mut fw = FwCfg::new();
        fw.add_file("etc/x", vec![1, 2, 3]);
        let mut mem = FakeMem::new(0x4000_0000, 0);
        fw.mmio_write(
            REG_SELECTOR,
            2,
            u64::from(KEY_FILE_DIR.swap_bytes()),
            &mut mem,
        );
        // FILE_DIR begins with a big-endian u32 file count == 1.
        let raw_count = fw.mmio_read(REG_DATA, 4);
        assert_eq!(
            u32::from_be_bytes((raw_count as u32).to_le_bytes()),
            1,
            "selector must resolve to FILE_DIR, not a bogus item"
        );
    }

    #[test]
    fn dma_read_moves_entry_into_guest_memory() {
        let mut fw = FwCfg::new();
        let mut mem = FakeMem::new(0x4000_0000, 0x1000);
        let dst = 0x4000_0100;
        let access = FwCfgDmaAccess {
            control: (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ,
            length: 4,
            address: dst,
        };
        let result = fw.dma_execute(access, &mut mem);
        assert_eq!(result, 0, "successful DMA clears all control bits");
        assert_eq!(mem.read_bytes(dst, 4).unwrap(), b"QEMU");
    }

    #[test]
    fn dma_read_unbacked_address_reports_error() {
        let mut fw = FwCfg::new();
        let mut mem = FakeMem::new(0x4000_0000, 0x10);
        let access = FwCfgDmaAccess {
            control: (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ,
            length: 4,
            address: 0x9999_0000, // outside the fake span
        };
        assert_eq!(fw.dma_execute(access, &mut mem), DMA_CTL_ERROR);
    }

    #[test]
    fn dma_write_into_readonly_entry_is_rejected() {
        let mut fw = FwCfg::new();
        let mut mem = FakeMem::new(0x4000_0000, 0x1000);
        mem.write_bytes(0x4000_0000, &[0xaa, 0xbb])
            .then_some(())
            .unwrap();
        fw.select(KEY_SIGNATURE);
        let access = FwCfgDmaAccess {
            control: DMA_CTL_WRITE,
            length: 2,
            address: 0x4000_0000,
        };
        assert_eq!(fw.dma_execute(access, &mut mem), DMA_CTL_ERROR);
    }

    #[test]
    fn dma_write_updates_writable_entry() {
        let mut fw = FwCfg::new();
        let sel = fw.add_writable_file("etc/system-states", vec![0, 0, 0, 0]);
        let mut mem = FakeMem::new(0x4000_0000, 0x1000);
        mem.write_bytes(0x4000_0000, &[1, 2, 3, 4]);
        let access = FwCfgDmaAccess {
            control: (u32::from(sel) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE,
            length: 4,
            address: 0x4000_0000,
        };
        assert_eq!(fw.dma_execute(access, &mut mem), 0);
        fw.select(sel);
        assert_eq!(fw.read_data(4), vec![1, 2, 3, 4]);
    }

    #[test]
    fn file_bytes_observes_named_entry_without_moving_cursor() {
        let mut fw = FwCfg::new();
        fw.add_file("etc/ramfb", vec![0xaa; 4]);
        fw.select(KEY_SIGNATURE);

        assert_eq!(fw.read_data_byte(), b'Q');
        assert_eq!(
            fw.file_bytes("etc/ramfb"),
            Some(&[0xaa, 0xaa, 0xaa, 0xaa][..])
        );
        assert_eq!(fw.read_data_byte(), b'E');
    }

    #[test]
    fn file_bytes_reflects_dma_write_to_writable_entry() {
        let mut fw = FwCfg::new();
        let sel = fw.add_writable_file("etc/ramfb", vec![0; 4]);
        let mut mem = FakeMem::new(0x4000_0000, 0x1000);
        mem.write_bytes(0x4000_0000, &[5, 6, 7, 8]);
        let access = FwCfgDmaAccess {
            control: (u32::from(sel) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE,
            length: 4,
            address: 0x4000_0000,
        };

        assert_eq!(fw.dma_execute(access, &mut mem), 0);

        assert_eq!(fw.file_bytes("etc/ramfb"), Some(&[5, 6, 7, 8][..]));
    }

    #[test]
    fn dma_skip_advances_cursor() {
        let mut fw = FwCfg::new();
        let mut mem = FakeMem::new(0x4000_0000, 0x10);
        fw.select(KEY_SIGNATURE);
        let access = FwCfgDmaAccess {
            control: DMA_CTL_SKIP,
            length: 2,
            address: 0,
        };
        assert_eq!(fw.dma_execute(access, &mut mem), 0);
        // Skipped "QE", next byte is 'M'.
        assert_eq!(fw.read_data_byte(), b'M');
    }
}

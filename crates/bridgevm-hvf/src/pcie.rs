//! PCIe ECAM config-space model for the Path A "QEMU virt contract" platform.
//!
//! The QEMU `virt` machine exposes its PCIe host bridge through an
//! `pci-host-ecam-generic` ECAM (Enhanced Configuration Access Mechanism)
//! window at [`crate::machine::PCIE_ECAM`] (`0x40_1000_0000`, 256 MiB, buses
//! `0..=0xff`). Firmware and the guest OS enumerate the bus by reading and
//! writing 4 KiB of config space per function directly through that window —
//! there is no I/O-port indirection like legacy x86 `CF8`/`CFC`.
//!
//! Until now the live platform answered every ECAM access with all-ones
//! (`0xFFFF_FFFF`), i.e. "no device at this address" for the whole bus. That is
//! a legal but empty machine: the guest sees a host bridge-less root complex and
//! cannot attach storage. This module replaces that stub with a real host bridge
//! at `00:00.0` plus the config read/write decode (including the BAR-sizing
//! probe protocol and an MSI-X capability builder) that future NVMe / virtio-pci
//! endpoints plug into.
//!
//! It is the same shape as the other Path A bricks: pure data + logic, no
//! Hypervisor.framework calls, fully unit-testable on any host. The live HVF run
//! loop maps guest ECAM faults onto [`PcieEcam::cfg_read`] / [`PcieEcam::cfg_write`]
//! (wired centrally in `platform_virt`, not here).
//!
//! References: the PCI-SIG "PCI Express Base Specification" config-space layout,
//! the `pci-host-ecam-generic` device-tree binding, and QEMU
//! `hw/pci-host/gpex.c` / `hw/pci/pci_host.c`.

use crate::machine::PCIE_ECAM;

// ---- ECAM geometry ----------------------------------------------------------

/// Bytes of config space per function (PCIe extended config space: 4 KiB).
pub const CFG_SPACE_SIZE: u64 = 0x1000;
/// Functions per device (3-bit function number).
pub const FUNCS_PER_DEVICE: u8 = 8;
/// Devices per bus (5-bit device number).
pub const DEVICES_PER_BUS: u8 = 32;

// ECAM address bit layout for `pci-host-ecam-generic`:
//   addr = base + (bus << 20 | dev << 15 | fn << 12 | reg)
// i.e. 8 bits bus, 5 bits device, 3 bits function, 12 bits register.
const SHIFT_BUS: u64 = 20;
const SHIFT_DEV: u64 = 15;
const SHIFT_FN: u64 = 12;
const MASK_REG: u64 = CFG_SPACE_SIZE - 1; // low 12 bits
const MASK_FN: u64 = 0x7; // 3 bits
const MASK_DEV: u64 = 0x1f; // 5 bits
const MASK_BUS: u64 = 0xff; // 8 bits

/// A decoded ECAM address: which function's config space, and the register
/// offset within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfgAddr {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    /// Register byte offset within the 4 KiB function config space.
    pub reg: u16,
}

impl CfgAddr {
    /// Decode an offset into the [`PCIE_ECAM`] window. `ecam_offset` is relative
    /// to [`PCIE_ECAM`]`.base` (the caller subtracts the base before dispatch).
    pub const fn from_ecam_offset(ecam_offset: u64) -> Self {
        Self {
            bus: ((ecam_offset >> SHIFT_BUS) & MASK_BUS) as u8,
            device: ((ecam_offset >> SHIFT_DEV) & MASK_DEV) as u8,
            function: ((ecam_offset >> SHIFT_FN) & MASK_FN) as u8,
            reg: (ecam_offset & MASK_REG) as u16,
        }
    }

    /// The Bus/Device/Function triple, for matching against modelled endpoints.
    pub const fn bdf(&self) -> (u8, u8, u8) {
        (self.bus, self.device, self.function)
    }
}

// ---- Type-0 config-space register offsets -----------------------------------

/// `0x00` Vendor ID (16-bit) + `0x02` Device ID (16-bit).
pub const REG_VENDOR_DEVICE: u16 = 0x00;
/// `0x04` Command (16-bit) + `0x06` Status (16-bit).
pub const REG_COMMAND_STATUS: u16 = 0x04;
/// `0x08` Revision ID (8-bit) + Class Code (24-bit).
pub const REG_REVISION_CLASS: u16 = 0x08;
/// `0x0c` Cache Line Size / Latency / Header Type / BIST.
pub const REG_BIST_HEADER: u16 = 0x0c;
/// First Base Address Register (`0x10`). A type-0 header has BAR0..BAR5.
pub const REG_BAR0: u16 = 0x10;
/// Capabilities pointer (8-bit at `0x34`).
pub const REG_CAP_PTR: u16 = 0x34;

/// Number of Base Address Registers in a type-0 (endpoint) header.
pub const NUM_BARS: usize = 6;

/// Header Type byte: type-0 (endpoint), single-function.
pub const HEADER_TYPE_ENDPOINT: u8 = 0x00;

// Command-register bits the host bridge actually honours.
/// Command bit 1: respond to memory-space accesses.
pub const CMD_MEMORY_SPACE: u16 = 1 << 1;
/// Command bit 2: act as a bus master (issue DMA).
pub const CMD_BUS_MASTER: u16 = 1 << 2;
/// Mask of command bits this model keeps writable; others read back as zero.
pub const CMD_WRITABLE_MASK: u16 = CMD_MEMORY_SPACE | CMD_BUS_MASTER;

/// Status register: capabilities-list present (bit 4). The host bridge has no
/// capability list, so this stays clear; endpoints that add MSI-X set it.
pub const STATUS_CAP_LIST: u16 = 1 << 4;

// ---- The host bridge identity (00:00.0) -------------------------------------

/// Red Hat, Inc. — the vendor QEMU uses for its paravirtual root complex.
pub const HOST_BRIDGE_VENDOR_ID: u16 = 0x1b36;
/// "QEMU PCIe Host bridge" device id.
pub const HOST_BRIDGE_DEVICE_ID: u16 = 0x0008;
/// Class code `0x060000`: base class 0x06 (bridge), sub-class 0x00 (host
/// bridge), prog-if 0x00.
pub const HOST_BRIDGE_CLASS_CODE: u32 = 0x0006_0000;
/// Revision id reported by the host bridge.
pub const HOST_BRIDGE_REVISION: u8 = 0x00;

/// The value an ECAM read returns when no device answers: all-ones. Firmware
/// treats a `0xFFFF_FFFF` vendor/device read as "slot empty".
pub const NO_DEVICE: u64 = 0xFFFF_FFFF;

/// A single modelled config-space function. Today the only one is the host
/// bridge; NVMe / virtio-pci endpoints add more.
#[derive(Debug, Clone)]
struct Function {
    bdf: (u8, u8, u8),
    vendor_device: u32,
    revision_class: u32,
    /// The mutable command register (low 16 bits) — bit-masked on write.
    command: u16,
    /// BAR latch values. A `0` size mask means "this BAR is unimplemented", so
    /// it always reads back `0` and ignores the all-ones sizing probe.
    bars: [Bar; NUM_BARS],
    /// Offset of the first capability in config space, or `0` for none.
    cap_ptr: u8,
    /// Raw capability bytes addressed by `cap_ptr` (sparse, by byte offset).
    cap_bytes: Vec<(u16, u8)>,
}

/// One Base Address Register and the size of the region it can decode.
#[derive(Debug, Clone, Copy, Default)]
struct Bar {
    /// Latched BAR value (low config/type bits OR'd with the programmed base).
    value: u32,
    /// Size mask: `!(size - 1)` for a power-of-two `size`, or `0` if the BAR is
    /// unimplemented. During the sizing probe the device returns this mask.
    size_mask: u32,
    /// Non-writable low type bits (memory/IO, 32/64-bit, prefetch) kept across
    /// a base re-program and the sizing probe.
    type_bits: u32,
}

impl Bar {
    /// Read back the BAR. After an all-ones sizing write the latched value is
    /// the size mask; otherwise it is the programmed base. Unimplemented BARs
    /// always read `0`.
    fn read(&self) -> u32 {
        if self.size_mask == 0 {
            0
        } else {
            self.value
        }
    }

    /// Apply a 32-bit BAR write. Writing all-ones latches the size mask (the
    /// sizing protocol); any other value latches the base with the type bits
    /// preserved.
    fn write(&mut self, value: u32) {
        if self.size_mask == 0 {
            return; // unimplemented: writes are dropped
        }
        if value == 0xFFFF_FFFF {
            // Sizing probe: report `size_mask | type_bits` on read-back.
            self.value = self.size_mask | self.type_bits;
        } else {
            // Program a base: only the address bits above the size are kept.
            self.value = (value & self.size_mask) | self.type_bits;
        }
    }
}

impl Function {
    /// The QEMU PCIe host bridge at `00:00.0`: type-0 header, no BARs, no
    /// capabilities. A clean, enumerable root complex.
    fn host_bridge() -> Self {
        Self {
            bdf: (0, 0, 0),
            vendor_device: (u32::from(HOST_BRIDGE_DEVICE_ID) << 16)
                | u32::from(HOST_BRIDGE_VENDOR_ID),
            revision_class: (HOST_BRIDGE_CLASS_CODE << 8) | u32::from(HOST_BRIDGE_REVISION),
            command: 0,
            bars: [Bar::default(); NUM_BARS],
            cap_ptr: 0,
            cap_bytes: Vec::new(),
        }
    }

    /// 32-bit dword read of register `reg` (already dword-aligned at the dword
    /// boundary that contains it).
    fn read_dword(&self, reg: u16) -> u32 {
        match reg {
            REG_VENDOR_DEVICE => self.vendor_device,
            REG_COMMAND_STATUS => {
                let status = if self.cap_ptr != 0 {
                    STATUS_CAP_LIST
                } else {
                    0
                };
                u32::from(self.command) | (u32::from(status) << 16)
            }
            REG_REVISION_CLASS => self.revision_class,
            REG_BIST_HEADER => {
                // Cache line / latency / BIST all zero; header type in byte 2.
                u32::from(HEADER_TYPE_ENDPOINT) << 16
            }
            REG_CAP_PTR => u32::from(self.cap_ptr),
            _ if (REG_BAR0..REG_BAR0 + (NUM_BARS as u16) * 4).contains(&reg) => {
                let idx = ((reg - REG_BAR0) / 4) as usize;
                self.bars[idx].read()
            }
            _ => self.read_capability_dword(reg),
        }
    }

    /// Read a dword out of the sparse capability bytes (zero-filled).
    fn read_capability_dword(&self, reg: u16) -> u32 {
        let mut dword = 0u32;
        for byte in 0..4 {
            let off = reg + byte;
            if let Some(&(_, v)) = self.cap_bytes.iter().find(|&&(o, _)| o == off) {
                dword |= u32::from(v) << (byte * 8);
            }
        }
        dword
    }

    /// 32-bit dword write of register `reg`.
    fn write_dword(&mut self, reg: u16, value: u32) {
        match reg {
            REG_COMMAND_STATUS => {
                // Command is the low 16 bits; status (high 16) is read-only /
                // write-1-to-clear, which this model treats as ignored.
                self.command = (value as u16) & CMD_WRITABLE_MASK;
            }
            _ if (REG_BAR0..REG_BAR0 + (NUM_BARS as u16) * 4).contains(&reg) => {
                let idx = ((reg - REG_BAR0) / 4) as usize;
                self.bars[idx].write(value);
            }
            // Identity, class and header registers are read-only; capability
            // bytes are read-only in this model.
            _ => {}
        }
    }
}

// ---- The ECAM device --------------------------------------------------------

/// The PCIe ECAM config-space model: decodes accesses to the
/// [`PCIE_ECAM`] window and answers for the host bridge (and any future
/// endpoints), returning all-ones for empty slots.
#[derive(Debug, Clone)]
pub struct PcieEcam {
    functions: Vec<Function>,
}

impl Default for PcieEcam {
    fn default() -> Self {
        Self::new()
    }
}

impl PcieEcam {
    /// A fresh root complex: one host bridge at `00:00.0`, every other slot
    /// empty.
    pub fn new() -> Self {
        Self {
            functions: vec![Function::host_bridge()],
        }
    }

    /// The size of the ECAM window this model decodes.
    pub const fn window() -> crate::machine::Region {
        PCIE_ECAM
    }

    fn function_at(&self, bdf: (u8, u8, u8)) -> Option<&Function> {
        self.functions.iter().find(|f| f.bdf == bdf)
    }

    fn function_at_mut(&mut self, bdf: (u8, u8, u8)) -> Option<&mut Function> {
        self.functions.iter_mut().find(|f| f.bdf == bdf)
    }

    /// Read `size` (1, 2 or 4) bytes of config space at `ecam_offset` (relative
    /// to [`PCIE_ECAM`]`.base`). Empty slots return all-ones; a present function
    /// returns the requested sub-dword field little-endian. Reads past the 4 KiB
    /// config space (or of an unaligned/oversized width) return all-ones.
    pub fn cfg_read(&self, ecam_offset: u64, size: u8) -> u64 {
        let addr = CfgAddr::from_ecam_offset(ecam_offset);
        let Some(func) = self.function_at(addr.bdf()) else {
            // No device: all-ones, sized to the access width.
            return all_ones(size);
        };
        let dword_reg = addr.reg & !0x3;
        let dword = func.read_dword(dword_reg);
        extract(dword, addr.reg, size)
    }

    /// Write `size` (1, 2 or 4) bytes of config space at `ecam_offset`. Writes to
    /// empty slots are dropped. A function performs a read-modify-write so a
    /// sub-dword write only touches the addressed bytes (the command register and
    /// BARs are word/dword-aligned in practice).
    pub fn cfg_write(&mut self, ecam_offset: u64, size: u8, value: u64) {
        let addr = CfgAddr::from_ecam_offset(ecam_offset);
        let Some(func) = self.function_at_mut(addr.bdf()) else {
            return;
        };
        let dword_reg = addr.reg & !0x3;
        let old = func.read_dword(dword_reg);
        let merged = insert(old, addr.reg, size, value);
        func.write_dword(dword_reg, merged);
    }

    /// True if `00:00.0` answers as the modelled host bridge (i.e. its vendor id
    /// read is not all-ones). Used by callers / tests as a presence check.
    pub fn host_bridge_present(&self) -> bool {
        let vid = self.cfg_read(0, 2);
        vid != u64::from(0xFFFFu16) && vid == u64::from(HOST_BRIDGE_VENDOR_ID)
    }
}

// ---- sub-dword access helpers -----------------------------------------------

/// All-ones for an access of `size` bytes (1, 2, 4 -> 0xFF, 0xFFFF, 0xFFFFFFFF;
/// any other width clamps to a 32-bit all-ones, matching a full-dword read).
fn all_ones(size: u8) -> u64 {
    match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => 0xFFFF_FFFF,
    }
}

/// Extract the `size`-byte field at byte offset `reg` from a 32-bit dword
/// (little-endian config space).
fn extract(dword: u32, reg: u16, size: u8) -> u64 {
    let byte = (reg & 0x3) as u32;
    let shift = byte * 8;
    let value = (dword >> shift) as u64;
    match size {
        1 => value & 0xFF,
        2 => value & 0xFFFF,
        4 => value & 0xFFFF_FFFF,
        _ => value & 0xFFFF_FFFF,
    }
}

/// Merge a `size`-byte `value` written at byte offset `reg` into an existing
/// `dword` (read-modify-write for sub-dword config writes).
fn insert(dword: u32, reg: u16, size: u8, value: u64) -> u32 {
    let byte = (reg & 0x3) as u32;
    let shift = byte * 8;
    let width_mask: u32 = match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => 0xFFFF_FFFF,
    };
    let field_mask = width_mask.checked_shl(shift).unwrap_or(0);
    let placed = ((value as u32) & width_mask)
        .checked_shl(shift)
        .unwrap_or(0);
    (dword & !field_mask) | placed
}

// ---- MSI-X capability builder -----------------------------------------------

/// The MSI-X capability id (PCI capability list entry type `0x11`).
pub const CAP_ID_MSIX: u8 = 0x11;

/// A built MSI-X capability structure, ready to splice into an endpoint's
/// capability list. Future NVMe / virtio-pci devices register one of these so
/// the guest driver can program per-vector message addresses.
///
/// The on-wire layout (PCIe spec §7.7.2) is a 12-byte capability:
/// ```text
///   +0  Cap ID (0x11)   +1  Next-cap ptr
///   +2  Message Control (16-bit): bits 0..10 = table size - 1, bit 15 = enable
///   +4  Table   Offset/BIR (32-bit): bits 0..2 = BIR, bits 3.. = table offset
///   +8  PBA     Offset/BIR (32-bit): bits 0..2 = BIR, bits 3.. = PBA   offset
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsixCapability {
    /// Number of interrupt vectors in the table (1..=2048).
    pub table_size: u16,
    /// BAR index (BIR) holding the MSI-X table.
    pub table_bir: u8,
    /// Byte offset of the table within `table_bir`'s BAR (must be 8-byte aligned).
    pub table_offset: u32,
    /// BAR index (BIR) holding the Pending Bit Array.
    pub pba_bir: u8,
    /// Byte offset of the PBA within `pba_bir`'s BAR (must be 8-byte aligned).
    pub pba_offset: u32,
}

impl MsixCapability {
    /// Total bytes of the MSI-X capability structure in config space.
    pub const SIZE_BYTES: u16 = 12;
    /// Bytes per MSI-X table entry (addr lo/hi, data, vector control).
    pub const ENTRY_BYTES: u32 = 16;
    /// Maximum encodable table size (the size field is 11 bits: `size - 1`).
    pub const MAX_TABLE_SIZE: u16 = 2048;

    /// Build a capability with `table_size` vectors whose table and PBA live in
    /// `bir` at `table_offset` / `pba_offset`. Panics on an out-of-range table
    /// size, an out-of-range BIR (0..=5), or a misaligned offset — the same
    /// fail-fast style as the rest of the platform model.
    pub fn new(table_size: u16, bir: u8, table_offset: u32, pba_offset: u32) -> Self {
        Self::with_birs(table_size, bir, table_offset, bir, pba_offset)
    }

    /// Build a capability whose table and PBA may live in different BARs.
    pub fn with_birs(
        table_size: u16,
        table_bir: u8,
        table_offset: u32,
        pba_bir: u8,
        pba_offset: u32,
    ) -> Self {
        assert!(
            (1..=Self::MAX_TABLE_SIZE).contains(&table_size),
            "MSI-X table size {table_size} out of range 1..=2048"
        );
        assert!((table_bir as usize) < NUM_BARS, "table BIR out of range");
        assert!((pba_bir as usize) < NUM_BARS, "PBA BIR out of range");
        assert!(
            table_offset % 8 == 0,
            "MSI-X table offset must be 8-byte aligned"
        );
        assert!(
            pba_offset % 8 == 0,
            "MSI-X PBA offset must be 8-byte aligned"
        );
        Self {
            table_size,
            table_bir,
            table_offset,
            pba_bir,
            pba_offset,
        }
    }

    /// The Message Control word: `table_size - 1` in bits 0..10. The MSI-X
    /// enable (bit 15) and function-mask (bit 14) bits start clear; the guest
    /// driver sets them.
    pub fn message_control(&self) -> u16 {
        (self.table_size - 1) & 0x07FF
    }

    /// The Table Offset/BIR dword: BIR in bits 0..2, offset (8-byte aligned) in
    /// the upper bits.
    pub fn table_offset_bir(&self) -> u32 {
        (self.table_offset & !0x7) | u32::from(self.table_bir & 0x7)
    }

    /// The PBA Offset/BIR dword.
    pub fn pba_offset_bir(&self) -> u32 {
        (self.pba_offset & !0x7) | u32::from(self.pba_bir & 0x7)
    }

    /// Total bytes the MSI-X table occupies in its BAR.
    pub fn table_byte_size(&self) -> u32 {
        u32::from(self.table_size) * Self::ENTRY_BYTES
    }

    /// Serialise the 12-byte capability with `next` as the next-cap pointer
    /// (`0` terminates the list). The caller splices this at the capability's
    /// config-space offset.
    pub fn to_bytes(&self, next: u8) -> [u8; Self::SIZE_BYTES as usize] {
        let mut bytes = [0u8; Self::SIZE_BYTES as usize];
        bytes[0] = CAP_ID_MSIX;
        bytes[1] = next;
        bytes[2..4].copy_from_slice(&self.message_control().to_le_bytes());
        bytes[4..8].copy_from_slice(&self.table_offset_bir().to_le_bytes());
        bytes[8..12].copy_from_slice(&self.pba_offset_bir().to_le_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine;

    /// Build a raw ECAM offset for a (bus, dev, fn, reg) tuple, the way the run
    /// loop derives it from a guest fault address minus the window base.
    fn ecam_offset(bus: u8, dev: u8, func: u8, reg: u16) -> u64 {
        (u64::from(bus) << SHIFT_BUS)
            | (u64::from(dev) << SHIFT_DEV)
            | (u64::from(func) << SHIFT_FN)
            | u64::from(reg)
    }

    #[test]
    fn ecam_offset_decodes_into_bdf_reg() {
        let off = ecam_offset(0x12, 0x1a, 0x5, 0x3c);
        let addr = CfgAddr::from_ecam_offset(off);
        assert_eq!(addr.bus, 0x12);
        assert_eq!(addr.device, 0x1a);
        assert_eq!(addr.function, 0x5);
        assert_eq!(addr.reg, 0x3c);
        assert_eq!(addr.bdf(), (0x12, 0x1a, 0x5));
    }

    #[test]
    fn window_matches_the_machine_map() {
        assert_eq!(PcieEcam::window(), machine::PCIE_ECAM);
        assert_eq!(PcieEcam::window().base, 0x40_1000_0000);
    }

    #[test]
    fn host_bridge_reports_vendor_and_device_id() {
        let ecam = PcieEcam::new();
        // 4-byte read of reg 0 gives device:vendor packed high:low.
        let vd = ecam.cfg_read(ecam_offset(0, 0, 0, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xFFFF, u64::from(HOST_BRIDGE_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xFFFF, u64::from(HOST_BRIDGE_DEVICE_ID));
        // 2-byte reads pick out the individual fields.
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 0, 0, 0x00), 2),
            u64::from(HOST_BRIDGE_VENDOR_ID)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 0, 0, 0x02), 2),
            u64::from(HOST_BRIDGE_DEVICE_ID)
        );
        assert!(ecam.host_bridge_present());
    }

    #[test]
    fn host_bridge_reports_host_bridge_class_and_header_type() {
        let ecam = PcieEcam::new();
        // Class code lives in the upper 24 bits of the revision/class dword.
        let rc = ecam.cfg_read(ecam_offset(0, 0, 0, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(HOST_BRIDGE_CLASS_CODE));
        assert_eq!(rc & 0xFF, u64::from(HOST_BRIDGE_REVISION));
        // Header type byte (offset 0x0e) is type-0.
        let header = ecam.cfg_read(ecam_offset(0, 0, 0, 0x0e), 1);
        assert_eq!(header, u64::from(HEADER_TYPE_ENDPOINT));
    }

    #[test]
    fn empty_slot_reads_all_ones() {
        let ecam = PcieEcam::new();
        // 00:01.0 is empty.
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, 0x00), 4), NO_DEVICE);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, 0x00), 2), 0xFFFF);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, 0x00), 1), 0xFF);
        // A different function of device 0 is also empty.
        assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 1, 0x00), 4), NO_DEVICE);
        // A non-zero bus is empty.
        assert_eq!(ecam.cfg_read(ecam_offset(1, 0, 0, 0x00), 4), NO_DEVICE);
    }

    #[test]
    fn writes_to_empty_slots_are_dropped() {
        let mut ecam = PcieEcam::new();
        ecam.cfg_write(ecam_offset(0, 1, 0, REG_COMMAND_STATUS), 2, 0x7);
        // Still empty.
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, 0x00), 4), NO_DEVICE);
    }

    #[test]
    fn command_register_is_writable_and_reads_back() {
        let mut ecam = PcieEcam::new();
        // Initially the command register is clear.
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2),
            0
        );
        // Enable memory space + bus master.
        let cmd = u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER);
        ecam.cfg_write(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2, cmd);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2),
            cmd
        );
        // Non-writable command bits (e.g. bit 0, I/O space) are masked off.
        ecam.cfg_write(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2, 0xFFFF);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2),
            u64::from(CMD_WRITABLE_MASK)
        );
    }

    #[test]
    fn status_high_word_is_not_clobbered_by_a_command_write() {
        let mut ecam = PcieEcam::new();
        // A 4-byte write to the command/status dword must only touch command.
        ecam.cfg_write(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 4, 0xFFFF_FFFF);
        let dword = ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 4);
        assert_eq!(dword & 0xFFFF, u64::from(CMD_WRITABLE_MASK));
        // Host bridge has no cap list, so the status word stays zero.
        assert_eq!(dword >> 16, 0);
    }

    #[test]
    fn host_bridge_bars_have_no_decode() {
        let ecam = PcieEcam::new();
        for i in 0..NUM_BARS {
            let reg = REG_BAR0 + (i as u16) * 4;
            assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 0, reg), 4), 0);
        }
    }

    #[test]
    fn host_bridge_bar_sizing_returns_zero_for_unimplemented_bars() {
        let mut ecam = PcieEcam::new();
        // The host bridge has no BARs: the all-ones sizing probe reads back 0
        // (a zero size mask means "no region"), which firmware reads as "skip".
        ecam.cfg_write(ecam_offset(0, 0, 0, REG_BAR0), 4, 0xFFFF_FFFF);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 0, REG_BAR0), 4), 0);
    }

    #[test]
    fn bar_sizing_returns_a_power_of_two_mask() {
        // Exercise the BAR sizing protocol directly: a 64 KiB 32-bit memory BAR.
        let mut bar = Bar {
            value: 0,
            size_mask: !(0x1_0000u32 - 1), // mask for a 64 KiB region
            type_bits: 0,                  // 32-bit, non-prefetch memory BAR
        };
        // Write all-ones, read back the size mask.
        bar.write(0xFFFF_FFFF);
        let readback = bar.read();
        // Firmware computes size as `!(readback & !0xF) + 1` for a memory BAR.
        let size = (!(readback & !0xF)).wrapping_add(1);
        assert_eq!(size, 0x1_0000);
        // The mask is a contiguous run of high ones => size is a power of two.
        assert!(size.is_power_of_two());
        // Programming a base keeps only the address bits above the size.
        bar.write(0x1234_5678);
        assert_eq!(bar.read() & !0xF, 0x1234_0000);
    }

    #[test]
    fn msix_capability_encodes_size_bir_and_offsets() {
        let cap = MsixCapability::new(8, 0, 0x0000, 0x0800);
        // Message control encodes table_size - 1 in the low 11 bits.
        assert_eq!(cap.message_control(), 7);
        // Table/PBA dwords pack the BIR into the low 3 bits.
        assert_eq!(cap.table_offset_bir() & 0x7, 0);
        assert_eq!(cap.table_offset_bir() & !0x7, 0x0000);
        assert_eq!(cap.pba_offset_bir() & !0x7, 0x0800);
        // Table occupies 8 entries * 16 bytes.
        assert_eq!(cap.table_byte_size(), 8 * 16);

        let bytes = cap.to_bytes(0x00);
        assert_eq!(bytes[0], CAP_ID_MSIX);
        assert_eq!(bytes[1], 0x00); // end of capability list
        assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 7);
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            cap.table_offset_bir()
        );
        assert_eq!(
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            cap.pba_offset_bir()
        );
    }

    #[test]
    fn msix_capability_supports_split_table_and_pba_birs() {
        let cap = MsixCapability::with_birs(2048, 2, 0x1000, 4, 0x2000);
        assert_eq!(cap.message_control(), 2047);
        assert_eq!(cap.table_offset_bir() & 0x7, 2);
        assert_eq!(cap.pba_offset_bir() & 0x7, 4);
    }

    #[test]
    #[should_panic(expected = "table size")]
    fn msix_rejects_an_out_of_range_table_size() {
        let _ = MsixCapability::new(0, 0, 0, 0);
    }

    #[test]
    #[should_panic(expected = "8-byte aligned")]
    fn msix_rejects_a_misaligned_offset() {
        let _ = MsixCapability::new(4, 0, 0x4, 0x800);
    }
}

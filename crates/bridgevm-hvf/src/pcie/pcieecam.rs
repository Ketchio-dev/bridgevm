//! Split out of pcie.rs to keep files under 850 lines.

use super::*;
use crate::machine::PCIE_ECAM;
use std::cell::Cell;

/// The PCIe ECAM config-space model: decodes accesses to the
/// [`PCIE_ECAM`] window and answers for the host bridge (and any future
/// endpoints), returning all-ones for empty slots.
#[derive(Debug, Clone)]
pub struct PcieEcam {
    pub(crate) functions: Vec<Function>,
    pub(crate) mmio_mru: Cell<Option<PcieMmioTargetMru>>,
}

/// A decoded memory-space access into a programmed PCI BAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcieMmioTarget {
    pub bdf: (u8, u8, u8),
    pub bar_index: usize,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcieMmioTargetMru {
    pub(crate) base: u64,
    pub(crate) end: u64,
    pub(crate) target: PcieMmioTarget,
}

impl PcieMmioTargetMru {
    pub(crate) fn target_for(self, gpa: u64) -> Option<PcieMmioTarget> {
        (self.base..self.end)
            .contains(&gpa)
            .then(|| PcieMmioTarget {
                offset: gpa - self.base,
                ..self.target
            })
    }
}

/// A decoded I/O-space access into a programmed PCI BAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciePioTarget {
    pub bdf: (u8, u8, u8),
    pub bar_index: usize,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PcieNvmeEndpointState {
    pub advertised: bool,
    pub command_memory_enabled: bool,
    pub command_bus_master_enabled: bool,
    pub bar0_assigned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcieEcamConfig {
    pub xhci_present: bool,
    pub hda_present: bool,
    pub virtio_blk_present: bool,
    pub virtio_net_present: bool,
    pub virtio_gpu_present: bool,
    pub virtio_console_present: bool,
    pub virtio_gpu_pci_device_id: u16,
    pub virtio_gpu_3d_enabled: bool,
}

impl Default for PcieEcamConfig {
    fn default() -> Self {
        Self {
            xhci_present: true,
            hda_present: false,
            virtio_blk_present: true,
            virtio_net_present: false,
            virtio_gpu_present: false,
            virtio_console_present: false,
            virtio_gpu_pci_device_id: VIRTIO_GPU_DEVICE_ID,
            virtio_gpu_3d_enabled: false,
        }
    }
}

impl Default for PcieEcam {
    fn default() -> Self {
        Self::new()
    }
}

impl PcieEcam {
    /// A fresh root complex: one host bridge at `00:00.0`, one NVMe endpoint at
    /// `00:01.0`, and the QEMU-oracle installer media endpoint at `00:03.0`.
    pub fn new() -> Self {
        Self::new_with_config(PcieEcamConfig::default())
    }

    pub fn new_with_config(config: PcieEcamConfig) -> Self {
        let mut functions = vec![Function::host_bridge(), Function::nvme()];
        if config.xhci_present {
            functions.push(Function::xhci());
        }
        if config.hda_present {
            functions.push(Function::hda());
        }
        if config.virtio_blk_present {
            functions.push(Function::virtio_blk());
        }
        if config.virtio_net_present {
            functions.push(Function::virtio_net());
        }
        if config.virtio_gpu_present {
            let host_visible_bar_size = config
                .virtio_gpu_3d_enabled
                .then(parse_virtio_gpu_hostmem_size)
                .filter(|size| *size != 0);
            functions.push(Function::virtio_gpu(
                host_visible_bar_size,
                config.virtio_gpu_pci_device_id,
            ));
        }
        if config.virtio_console_present {
            functions.push(Function::virtio_console());
        }
        Self {
            functions,
            mmio_mru: Cell::new(None),
        }
    }

    /// The size of the ECAM window this model decodes.
    pub const fn window() -> crate::machine::Region {
        PCIE_ECAM
    }

    pub(crate) fn function_at(&self, bdf: (u8, u8, u8)) -> Option<&Function> {
        self.functions.iter().find(|f| f.bdf == bdf)
    }

    pub(crate) fn function_at_mut(&mut self, bdf: (u8, u8, u8)) -> Option<&mut Function> {
        self.functions.iter_mut().find(|f| f.bdf == bdf)
    }

    pub fn nvme_endpoint_state(&self) -> PcieNvmeEndpointState {
        let Some(func) = self.function_at(NVME_BDF) else {
            return PcieNvmeEndpointState::default();
        };
        let expected_vendor_device = (u32::from(NVME_DEVICE_ID) << 16) | u32::from(NVME_VENDOR_ID);
        let expected_revision_class = (NVME_CLASS_CODE << 8) | u32::from(NVME_REVISION);
        PcieNvmeEndpointState {
            advertised: func.vendor_device == expected_vendor_device
                && func.revision_class == expected_revision_class,
            command_memory_enabled: func.command & CMD_MEMORY_SPACE != 0,
            command_bus_master_enabled: func.command & CMD_BUS_MASTER != 0,
            bar0_assigned: func.bars[0].assigned_base().is_some(),
        }
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
        let value = extract(dword, addr.reg, size);
        if addr.bdf() == VIRTIO_GPU_BDF {
            venus_start_trace_cfg("cfg_read", addr.reg, size, value);
        }
        value
    }

    /// Write `size` (1, 2 or 4) bytes of config space at `ecam_offset`. Writes to
    /// empty slots are dropped. A function performs a read-modify-write so a
    /// sub-dword write only touches the addressed bytes (the command register and
    /// BARs are word/dword-aligned in practice).
    pub fn cfg_write(&mut self, ecam_offset: u64, size: u8, value: u64) {
        self.mmio_mru.set(None);
        let addr = CfgAddr::from_ecam_offset(ecam_offset);
        if addr.bdf() == VIRTIO_GPU_BDF {
            venus_start_trace_cfg("cfg_write", addr.reg, size, value);
        }
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

    /// Resolve an absolute guest-physical address in PCI memory space to the
    /// programmed endpoint BAR that decodes it. Only functions with Memory Space
    /// enabled in the PCI command register are allowed to answer.
    pub fn mmio_target(&self, gpa: u64) -> Option<PcieMmioTarget> {
        if let Some(mru) = self.mmio_mru.get() {
            if let Some(target) = mru.target_for(gpa) {
                return Some(target);
            }
        }
        for func in &self.functions {
            if func.command & CMD_MEMORY_SPACE == 0 {
                continue;
            }
            for idx in 0..func.bars.len() {
                if let Some(mru) = func.mmio_target_of_bar(idx, gpa) {
                    self.mmio_mru.set(Some(mru));
                    return Some(mru.target);
                }
            }
        }
        None
    }

    /// Resolve a PCI I/O-port address to the programmed endpoint BAR that
    /// decodes it. Only functions with I/O Space enabled in the command register
    /// are allowed to answer.
    pub fn pio_target(&self, port: u64) -> Option<PciePioTarget> {
        for func in &self.functions {
            if func.command & CMD_IO_SPACE == 0 {
                continue;
            }
            for (idx, bar) in func.bars.iter().enumerate() {
                if let Some(offset) = bar.pio_offset_of(port) {
                    return Some(PciePioTarget {
                        bdf: func.bdf,
                        bar_index: idx,
                        offset,
                    });
                }
            }
        }
        None
    }

    /// Function-level MSI-X control for the first NVMe endpoint.
    pub fn nvme_msix_control(&self) -> MsixFunctionControl {
        self.function_at(NVME_BDF)
            .and_then(Function::msix_control)
            .unwrap_or_default()
    }

    /// Function-level MSI-X control for the xHCI endpoint.
    pub fn xhci_msix_control(&self) -> MsixFunctionControl {
        self.function_at(XHCI_BDF)
            .and_then(Function::msix_control)
            .unwrap_or_default()
    }

    /// Standard MSI programming for the opt-in HDA endpoint.
    pub fn hda_msi_config(&self) -> HdaMsiConfig {
        self.function_at(HDA_BDF)
            .and_then(Function::msi_config)
            .unwrap_or_default()
    }

    /// Function-level MSI-X control for the virtio-net endpoint.
    pub fn virtio_net_msix_control(&self) -> MsixFunctionControl {
        self.function_at(VIRTIO_NET_BDF)
            .and_then(Function::msix_control)
            .unwrap_or_default()
    }

    /// Function-level MSI-X control for the virtio-gpu endpoint.
    pub fn virtio_gpu_msix_control(&self) -> MsixFunctionControl {
        self.function_at(VIRTIO_GPU_BDF)
            .and_then(Function::msix_control)
            .unwrap_or_default()
    }

    /// Function-level MSI-X control for the virtio-console endpoint.
    pub fn virtio_console_msix_control(&self) -> MsixFunctionControl {
        self.function_at(VIRTIO_CONSOLE_BDF)
            .and_then(Function::msix_control)
            .unwrap_or_default()
    }

    pub fn virtio_gpu_host_visible_bar_base(&self) -> Option<u64> {
        let func = self.function_at(VIRTIO_GPU_BDF)?;
        func.memory64_assigned_base(2)
    }

    pub fn virtio_gpu_host_visible_bar_size(&self) -> Option<u64> {
        self.function_at(VIRTIO_GPU_BDF)
            .map(|func| func.bars[2].size())
            .filter(|size| *size != 0)
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(self.functions.len() as u32);

        for function in &self.functions {
            out.write_u8(function.bdf.0);
            out.write_u8(function.bdf.1);
            out.write_u8(function.bdf.2);
            out.write_u8(0);
            out.write_u16(function.command);
            out.write_u16(0);

            for bar in &function.bars {
                out.write_u32(bar.value);
            }

            out.write_u32(function.cap_bytes.len() as u32);
            for &(offset, value) in &function.cap_bytes {
                out.write_u16(offset);
                out.write_u8(value);
                out.write_u8(0);
            }
        }

        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(input.read_u32(), 1, "unsupported PCIe snapshot version");
        assert_eq!(
            input.read_u32() as usize,
            self.functions.len(),
            "PCIe function-count mismatch on restore"
        );

        for function in &mut self.functions {
            let bdf = (input.read_u8(), input.read_u8(), input.read_u8());
            assert_eq!(input.read_u8(), 0, "invalid PCIe snapshot");
            assert_eq!(bdf, function.bdf, "PCIe BDF mismatch on restore");

            function.command = input.read_u16() & CMD_WRITABLE_MASK;
            assert_eq!(input.read_u16(), 0, "invalid PCIe snapshot");

            for bar in &mut function.bars {
                bar.value = input.read_u32();
            }

            let capability_count = input.read_u32() as usize;
            assert_eq!(
                capability_count,
                function.cap_bytes.len(),
                "PCIe capability shape mismatch on restore"
            );
            for capability in &mut function.cap_bytes {
                let offset = input.read_u16();
                let value = input.read_u8();
                assert_eq!(input.read_u8(), 0, "invalid PCIe snapshot");
                assert_eq!(offset, capability.0, "PCIe capability offset mismatch");
                capability.1 = value;
            }
        }

        self.mmio_mru.set(None);
        input.finish();
    }
}

pub fn parse_virtio_gpu_hostmem_size() -> u64 {
    let mib = std::env::var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1024);
    if mib == 0 {
        return 0;
    }
    assert!(
        mib.is_power_of_two(),
        "BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB must be a power of two"
    );
    let size = mib
        .checked_mul(1024 * 1024)
        .expect("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB overflows bytes");
    assert!(
        size <= u64::from(u32::MAX),
        "BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB must be less than 4096 for this BAR model"
    );
    size
}

// ---- sub-dword access helpers -----------------------------------------------

/// All-ones for an access of `size` bytes (1, 2, 4 -> 0xFF, 0xFFFF, 0xFFFFFFFF;
/// any other width clamps to a 32-bit all-ones, matching a full-dword read).
pub(crate) fn all_ones(size: u8) -> u64 {
    match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => 0xFFFF_FFFF,
    }
}

/// Extract the `size`-byte field at byte offset `reg` from a 32-bit dword
/// (little-endian config space).
pub(crate) fn extract(dword: u32, reg: u16, size: u8) -> u64 {
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
pub(crate) fn insert(dword: u32, reg: u16, size: u8, value: u64) -> u32 {
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

/// The standard MSI capability id (PCI capability list entry type `0x05`).
pub const CAP_ID_MSI: u8 = 0x05;
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

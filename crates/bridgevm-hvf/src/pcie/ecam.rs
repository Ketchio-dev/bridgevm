//! The PcieEcam root complex: configuration, function lookup, config read/write.

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
}

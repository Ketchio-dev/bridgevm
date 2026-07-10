// allow: SIZE_OK - Task 5q PCIe ECAM model is a legacy monolithic surface carried to preserve validated HVF/PCIe evidence; full modular split is separate work.
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
//! a legal but empty machine: the guest sees a storage-less root complex and
//! cannot attach install media. This module replaces that stub with a real host
//! bridge at `00:00.0`, an NVMe endpoint at `00:01.0`, and the config
//! read/write decode (including the BAR-sizing probe protocol and an MSI-X
//! capability builder) that further PCIe endpoints plug into.
//!
//! It is the same shape as the other Path A bricks: pure data + logic, no
//! Hypervisor.framework calls, fully unit-testable on any host. The live HVF run
//! loop maps guest ECAM faults onto [`PcieEcam::cfg_read`] / [`PcieEcam::cfg_write`]
//! (wired centrally in `platform_virt`, not here).
//!
//! References: the PCI-SIG "PCI Express Base Specification" config-space layout,
//! the `pci-host-ecam-generic` device-tree binding, and QEMU
//! `hw/pci-host/gpex.c` / `hw/pci/pci_host.c`.

use std::cell::Cell;

use crate::machine::PCIE_ECAM;

mod virtio_caps;

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
pub const REG_SUBSYSTEM_IDS: u16 = 0x2c;

/// Number of Base Address Registers in a type-0 (endpoint) header.
pub const NUM_BARS: usize = 6;

/// Header Type byte: type-0 (endpoint), single-function.
pub const HEADER_TYPE_ENDPOINT: u8 = 0x00;

// Command-register bits the host bridge actually honours.
/// Command bit 0: respond to I/O-space accesses.
pub const CMD_IO_SPACE: u16 = 1 << 0;
/// Command bit 1: respond to memory-space accesses.
pub const CMD_MEMORY_SPACE: u16 = 1 << 1;
/// Command bit 2: act as a bus master (issue DMA).
pub const CMD_BUS_MASTER: u16 = 1 << 2;
/// Mask of command bits this model keeps writable; others read back as zero.
pub const CMD_WRITABLE_MASK: u16 = CMD_IO_SPACE | CMD_MEMORY_SPACE | CMD_BUS_MASTER;

/// Status register: capabilities-list present (bit 4). The host bridge has no
/// capability list, so this stays clear; endpoints that add MSI-X set it.
pub const STATUS_CAP_LIST: u16 = 1 << 4;

/// Function-level MSI-X control bits from the PCI capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MsixFunctionControl {
    pub enabled: bool,
    pub function_masked: bool,
}

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

// ---- The first storage endpoint (00:01.0) -----------------------------------

/// Bus/device/function used by the first NVMe controller. QEMU command lines
/// commonly attach the first user device at slot 1, leaving `00:00.0` for the
/// host bridge.
pub const NVME_BDF: (u8, u8, u8) = (0, 1, 0);
/// Vendor id used by QEMU's PCIe devices.
pub const NVME_VENDOR_ID: u16 = 0x1b36;
/// Device id for the BridgeVM/QEMU-style NVMe endpoint.
pub const NVME_DEVICE_ID: u16 = 0x0010;
/// Class code `0x010802`: mass storage / NVM Express / NVMe.
pub const NVME_CLASS_CODE: u32 = 0x0001_0802;
/// Revision id reported by the endpoint.
pub const NVME_REVISION: u8 = 0x01;
/// BAR0 window for controller registers and doorbells. 16 KiB covers the
/// register page plus enough doorbells for the small queue count this VMM
/// exposes; it is power-of-two for PCI BAR sizing.
pub const NVME_BAR0_SIZE: u32 = 0x4000;
/// PCI capability-list offset for the NVMe endpoint's MSI-X capability.
pub const NVME_MSIX_CAP_OFFSET: u8 = 0x40;
/// NVMe subsystem IDs matching QEMU's `nvme` device (subsystem vendor 0x1af4,
/// subsystem id 0x1100), which EDK2 boots.
pub const NVME_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const NVME_SUBSYSTEM_ID: u16 = 0x1100;
/// Power Management capability offset chained between MSI-X and PCI Express.
/// QEMU's NVMe (which EDK2 boots) exposes a PM capability; EDK2 may power the
/// endpoint to D0 through it before the driver touches the controller.
pub const NVME_PM_CAP_OFFSET: u8 = 0x50;
/// Minimal PCI Power Management capability (ID 0x01): PMC version 3, PMCSR in
/// D0. The `next` byte is patched to chain onward when the endpoint is built.
const NVME_PM_CAP_BYTES: [u8; 8] = [0x01, 0x00, 0x03, 0x00, 0x08, 0x00, 0x00, 0x00];
/// PCI Express capability offset chained after the NVMe Power Management
/// capability. NVMe is a PCIe endpoint; EDK2's NvmExpressDxe only binds a device
/// that advertises a PCI Express capability (our xHCI endpoint has one and EDK2
/// binds it, QEMU's NVMe has one and EDK2 boots it), so the NVMe endpoint must
/// expose one too.
pub const NVME_PCIE_CAP_OFFSET: u8 = 0x60;
/// Number of MSI-X vectors exposed by the NVMe endpoint: one admin vector plus
/// eight I/O vectors so SMP guests can spread storage completions across vCPUs.
pub const NVME_MSIX_VECTOR_COUNT: u16 = 9;
/// Offset of the MSI-X table in BAR0. Kept away from NVMe registers/doorbells.
pub const NVME_MSIX_TABLE_OFFSET: u32 = 0x2000;
/// Offset of the MSI-X Pending Bit Array in BAR0.
pub const NVME_MSIX_PBA_OFFSET: u32 = 0x3000;

// ---- The QEMU xHCI controller endpoint (00:02.0) ---------------------------

/// Bus/device/function QEMU uses for `qemu-xhci` in the Windows installer oracle.
pub const XHCI_BDF: (u8, u8, u8) = (0, 2, 0);
/// Vendor id used by QEMU's xHCI controller.
pub const XHCI_VENDOR_ID: u16 = 0x1b36;
/// Device id for QEMU's `qemu-xhci` endpoint.
pub const XHCI_DEVICE_ID: u16 = 0x000d;
/// Class code `0x0c0330`: serial bus / USB / xHCI.
pub const XHCI_CLASS_CODE: u32 = 0x000c_0330;
/// Revision id reported by QEMU's `qemu-xhci`.
pub const XHCI_REVISION: u8 = 0x01;
pub const XHCI_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const XHCI_SUBSYSTEM_ID: u16 = 0x1100;
/// QEMU's 64-bit xHCI memory BAR size.
pub const XHCI_BAR0_SIZE: u32 = 0x4000;
/// PCI capability-list offset for the xHCI endpoint's MSI-X capability.
pub const XHCI_MSIX_CAP_OFFSET: u8 = 0x90;
/// PCI Express capability offset following the MSI-X capability.
pub const XHCI_PCIE_CAP_OFFSET: u8 = 0xa0;
/// Number of MSI-X vectors exposed by QEMU's xHCI endpoint.
pub const XHCI_MSIX_VECTOR_COUNT: u16 = 16;
/// Offset of the xHCI MSI-X table in BAR0.
pub const XHCI_MSIX_TABLE_OFFSET: u32 = 0x3000;
/// Offset of the xHCI MSI-X Pending Bit Array in BAR0.
pub const XHCI_MSIX_PBA_OFFSET: u32 = 0x3800;
const XHCI_PCIE_CAP_BYTES: [u8; 20] = [
    0x10, 0x00, 0x92, 0x00, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x04, 0x00, 0x00,
    0x00, 0x00, 0x11, 0x00,
];

// ---- The QEMU-oracle installer media endpoint (00:03.0) --------------------

/// Bus/device/function QEMU uses for the Windows installer media device in the
/// live GICv3 oracle (`virtio-blk-pci` behind the PCI root).
pub const VIRTIO_BLK_BDF: (u8, u8, u8) = (0, 3, 0);
/// Red Hat virtio vendor id.
pub const VIRTIO_BLK_VENDOR_ID: u16 = 0x1af4;
/// Transitional virtio block device id reported by QEMU's `virtio-blk-pci`.
pub const VIRTIO_BLK_DEVICE_ID: u16 = 0x1001;
/// Class code `0x010000`: mass storage / SCSI storage controller.
pub const VIRTIO_BLK_CLASS_CODE: u32 = 0x0001_0000;
/// Revision id reported by the boot-media endpoint.
pub const VIRTIO_BLK_REVISION: u8 = 0x00;
pub const VIRTIO_BLK_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const VIRTIO_BLK_SUBSYSTEM_ID: u16 = 0x0002;
/// Legacy virtio-blk-pci I/O BAR.
pub const VIRTIO_BLK_BAR0_SIZE: u32 = 0x80;
/// MSI-X table/PBA memory BAR.
pub const VIRTIO_BLK_BAR1_SIZE: u32 = 0x1000;
/// Modern virtio PCI transport memory BAR.
pub const VIRTIO_BLK_BAR4_SIZE: u32 = 0x4000;
/// PCI capability-list offset for the virtio-blk MSI-X capability.
pub const VIRTIO_BLK_MSIX_CAP_OFFSET: u8 = 0x84;
/// Number of MSI-X vectors exposed by the boot-media endpoint.
pub const VIRTIO_BLK_MSIX_VECTOR_COUNT: u16 = 2;
/// Offset of the virtio-blk MSI-X table in BAR1.
pub const VIRTIO_BLK_MSIX_TABLE_OFFSET: u32 = 0x0000;
/// Offset of the virtio-blk MSI-X Pending Bit Array in BAR1.
pub const VIRTIO_BLK_MSIX_PBA_OFFSET: u32 = 0x0800;

// ---- The modern virtio-net endpoint (00:04.0) ------------------------------

/// Bus/device/function for the opt-in modern-only `virtio-net-pci` endpoint.
pub const VIRTIO_NET_BDF: (u8, u8, u8) = (0, 4, 0);
/// Red Hat virtio vendor id.
pub const VIRTIO_NET_VENDOR_ID: u16 = 0x1af4;
/// Modern virtio network device id (`0x1040 + virtio device id 1`).
pub const VIRTIO_NET_DEVICE_ID: u16 = 0x1041;
/// Class code `0x020000`: network / Ethernet controller.
pub const VIRTIO_NET_CLASS_CODE: u32 = 0x0002_0000;
/// Modern virtio PCI revision id.
pub const VIRTIO_NET_REVISION: u8 = 0x01;
pub const VIRTIO_NET_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const VIRTIO_NET_SUBSYSTEM_ID: u16 = 0x0040;
/// MSI-X table/PBA memory BAR.
pub const VIRTIO_NET_BAR1_SIZE: u32 = 0x1000;
/// Modern virtio PCI transport memory BAR.
pub const VIRTIO_NET_BAR4_SIZE: u32 = 0x4000;
/// PCI capability-list offset for the virtio-net MSI-X capability.
pub const VIRTIO_NET_MSIX_CAP_OFFSET: u8 = 0x84;
/// One vector per virtio-net queue (RX=0, TX=1).
pub const VIRTIO_NET_MSIX_VECTOR_COUNT: u16 = 2;
/// Offset of the virtio-net MSI-X table in BAR1.
pub const VIRTIO_NET_MSIX_TABLE_OFFSET: u32 = 0x0000;
/// Offset of the virtio-net MSI-X Pending Bit Array in BAR1.
pub const VIRTIO_NET_MSIX_PBA_OFFSET: u32 = 0x0800;

// ---- The modern virtio-gpu endpoint (00:05.0) ------------------------------

/// Bus/device/function for the opt-in modern-only `virtio-gpu-pci` endpoint.
pub const VIRTIO_GPU_BDF: (u8, u8, u8) = (0, 5, 0);
/// Red Hat virtio vendor id.
pub const VIRTIO_GPU_VENDOR_ID: u16 = 0x1af4;
/// Modern virtio GPU device id (`0x1040 + virtio device id 16`).
pub const VIRTIO_GPU_DEVICE_ID: u16 = 0x1050;
/// Class code `0x038000`: display controller / other.
pub const VIRTIO_GPU_CLASS_CODE: u32 = 0x0003_8000;
/// Modern virtio PCI revision id.
pub const VIRTIO_GPU_REVISION: u8 = 0x01;
pub const VIRTIO_GPU_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const VIRTIO_GPU_SUBSYSTEM_ID: u16 = 0x1100;
/// MSI-X table/PBA memory BAR.
pub const VIRTIO_GPU_BAR1_SIZE: u32 = 0x1000;
/// Host-visible virtio-gpu shared-memory BAR default size (1 GiB).
pub const VIRTIO_GPU_HOSTMEM_DEFAULT_SIZE: u64 = 1024 * 1024 * 1024;
pub const VIRTIO_GPU_SHM_ID_HOST_VISIBLE: u8 = 1;
/// Modern virtio PCI transport memory BAR.
pub const VIRTIO_GPU_BAR4_SIZE: u32 = 0x4000;
/// PCI capability-list offset for the virtio-gpu MSI-X capability.
pub const VIRTIO_GPU_MSIX_CAP_OFFSET: u8 = 0x84;
/// One vector per virtio-gpu queue (controlq=0, cursorq=1).
pub const VIRTIO_GPU_MSIX_VECTOR_COUNT: u16 = 2;
/// Offset of the virtio-gpu MSI-X table in BAR1.
pub const VIRTIO_GPU_MSIX_TABLE_OFFSET: u32 = 0x0000;
/// Offset of the virtio-gpu MSI-X Pending Bit Array in BAR1.
pub const VIRTIO_GPU_MSIX_PBA_OFFSET: u32 = 0x0800;

// ---- The modern virtio-console endpoint (00:06.0) --------------------------

/// Bus/device/function for the opt-in modern-only `virtio-serial-pci` endpoint.
pub const VIRTIO_CONSOLE_BDF: (u8, u8, u8) = (0, 6, 0);
/// Red Hat virtio vendor id.
pub const VIRTIO_CONSOLE_VENDOR_ID: u16 = 0x1af4;
/// Modern virtio console device id (`0x1040 + virtio device id 3`).
pub const VIRTIO_CONSOLE_DEVICE_ID: u16 = 0x1043;
/// Class code `0x078000`: simple communications controller / other.
pub const VIRTIO_CONSOLE_CLASS_CODE: u32 = 0x0007_8000;
/// Modern virtio PCI revision id.
pub const VIRTIO_CONSOLE_REVISION: u8 = 0x01;
pub const VIRTIO_CONSOLE_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const VIRTIO_CONSOLE_SUBSYSTEM_ID: u16 = 0x1100;
/// MSI-X table/PBA memory BAR.
pub const VIRTIO_CONSOLE_BAR1_SIZE: u32 = 0x1000;
/// Modern virtio PCI transport memory BAR.
pub const VIRTIO_CONSOLE_BAR4_SIZE: u32 = 0x4000;
/// PCI capability-list offset for the virtio-console MSI-X capability.
pub const VIRTIO_CONSOLE_MSIX_CAP_OFFSET: u8 = 0x84;
/// One vector per virtio-console queue.
pub const VIRTIO_CONSOLE_MSIX_VECTOR_COUNT: u16 = 6;
/// Offset of the virtio-console MSI-X table in BAR1.
pub const VIRTIO_CONSOLE_MSIX_TABLE_OFFSET: u32 = 0x0000;
/// Offset of the virtio-console MSI-X Pending Bit Array in BAR1.
pub const VIRTIO_CONSOLE_MSIX_PBA_OFFSET: u32 = 0x0800;

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
    subsystem_ids: u32,
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
    kind: BarKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum BarKind {
    #[default]
    Memory32,
    Memory64Low,
    Memory64High,
    Io,
}

impl Bar {
    /// Construct a 32-bit, non-prefetchable memory BAR with a power-of-two size.
    fn memory32(size: u32) -> Self {
        assert!(
            size >= 0x10,
            "PCI memory BAR size must be at least 16 bytes"
        );
        assert!(
            size.is_power_of_two(),
            "PCI memory BAR size must be a power of two"
        );
        Self {
            value: 0,
            size_mask: !(size - 1),
            type_bits: 0,
            kind: BarKind::Memory32,
        }
    }

    fn memory64(size: u32) -> (Self, Self) {
        Self::memory64_with_type_bits(size, 0x4)
    }

    fn memory64_prefetchable(size: u32) -> (Self, Self) {
        Self::memory64_with_type_bits(size, 0x0c)
    }

    fn memory64_with_type_bits(size: u32, low_type_bits: u32) -> (Self, Self) {
        assert!(
            size >= 0x10,
            "PCI memory BAR size must be at least 16 bytes"
        );
        assert!(
            size.is_power_of_two(),
            "PCI memory BAR size must be a power of two"
        );
        (
            Self {
                value: 0,
                size_mask: !(size - 1),
                type_bits: low_type_bits,
                kind: BarKind::Memory64Low,
            },
            Self {
                value: 0,
                size_mask: 0xFFFF_FFFF,
                type_bits: 0,
                kind: BarKind::Memory64High,
            },
        )
    }

    /// Construct an I/O BAR with a power-of-two size.
    fn io(size: u32) -> Self {
        assert!(size >= 0x4, "PCI I/O BAR size must be at least 4 bytes");
        assert!(
            size.is_power_of_two(),
            "PCI I/O BAR size must be a power of two"
        );
        Self {
            value: 0,
            size_mask: !(size - 1),
            type_bits: 0x1,
            kind: BarKind::Io,
        }
    }

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

    /// Size of the decoded BAR region, or zero if unimplemented.
    fn size(&self) -> u64 {
        if self.size_mask == 0 {
            0
        } else {
            let mask = match self.kind {
                BarKind::Memory32 | BarKind::Memory64Low => self.size_mask & !0xF,
                BarKind::Memory64High => return 0,
                BarKind::Io => self.size_mask & !0x3,
            };
            u64::from((!mask).wrapping_add(1))
        }
    }

    /// Programmed base, if the BAR is implemented.
    fn base(&self) -> Option<u64> {
        if self.size_mask == 0 {
            return None;
        }
        let mask = match self.kind {
            BarKind::Memory32 | BarKind::Memory64Low => !0xF,
            BarKind::Memory64High => return None,
            BarKind::Io => !0x3,
        };
        Some(u64::from(self.value & self.size_mask & mask))
    }

    fn assigned_base(&self) -> Option<u64> {
        let base = self.base()?;
        let sizing_readback = self.size_mask | self.type_bits;
        match self.kind {
            BarKind::Memory32 | BarKind::Memory64Low => {
                (base != 0 && self.value != sizing_readback).then_some(base)
            }
            BarKind::Io => (self.value != sizing_readback).then_some(base),
            BarKind::Memory64High => None,
        }
    }

    /// Offset into this BAR for `addr`, if the BAR currently decodes it.
    fn offset_of(&self, addr: u64) -> Option<u64> {
        let base = self.assigned_base()?;
        let size = self.size();
        let offset = addr.checked_sub(base)?;
        (offset < size).then_some(offset)
    }

    fn pio_offset_of(&self, port: u64) -> Option<u64> {
        (self.kind == BarKind::Io)
            .then(|| self.offset_of(port))
            .flatten()
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
            subsystem_ids: 0,
            command: 0,
            bars: [Bar::default(); NUM_BARS],
            cap_ptr: 0,
            cap_bytes: Vec::new(),
        }
    }

    /// The first NVMe storage endpoint at `00:01.0`.
    fn nvme() -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        // The NVMe spec requires the controller registers behind a 64-bit BAR
        // (BAR0/BAR1 pair). EDK2's NvmExpressDxe refuses to bind a 32-bit BAR0.
        // Expose a 64-bit BAR0 like the xHCI endpoint EDK2 binds, and hardwire
        // the low BAR's memory-type bits (bit 2 = 64-bit) into its read-back so
        // an un-programmed BAR0 reads 0x4 — matching QEMU's NVMe (which EDK2
        // boots) and the PCI spec, where those type bits are read-only rather
        // than only appearing during a sizing probe.
        let (mut bar0, bar1) = Bar::memory64(NVME_BAR0_SIZE);
        bar0.value = bar0.type_bits;
        bars[0] = bar0;
        bars[1] = bar1;
        let msix = MsixCapability::new(
            NVME_MSIX_VECTOR_COUNT,
            0,
            NVME_MSIX_TABLE_OFFSET,
            NVME_MSIX_PBA_OFFSET,
        );
        // Capability chain: MSI-X (0x40) -> Power Management (0x50) ->
        // PCI Express (0x60, terminates), mirroring QEMU's NVMe endpoint.
        let mut cap_bytes: Vec<(u16, u8)> = msix
            .to_bytes(NVME_PM_CAP_OFFSET)
            .into_iter()
            .enumerate()
            .map(|(i, byte)| (u16::from(NVME_MSIX_CAP_OFFSET) + i as u16, byte))
            .collect();
        let mut pm_cap = NVME_PM_CAP_BYTES;
        pm_cap[1] = NVME_PCIE_CAP_OFFSET;
        cap_bytes.extend(
            pm_cap
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(NVME_PM_CAP_OFFSET) + i as u16, byte)),
        );
        cap_bytes.extend(
            XHCI_PCIE_CAP_BYTES
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(NVME_PCIE_CAP_OFFSET) + i as u16, byte)),
        );
        Self {
            bdf: NVME_BDF,
            vendor_device: (u32::from(NVME_DEVICE_ID) << 16) | u32::from(NVME_VENDOR_ID),
            revision_class: (NVME_CLASS_CODE << 8) | u32::from(NVME_REVISION),
            // Match QEMU's NVMe subsystem IDs (1af4:1100); some enumerators
            // distrust a zero subsystem ID.
            subsystem_ids: (u32::from(NVME_SUBSYSTEM_ID) << 16)
                | u32::from(NVME_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: NVME_MSIX_CAP_OFFSET,
            cap_bytes,
        }
    }

    /// QEMU-oracle virtio block installer media endpoint at `00:03.0`.
    fn virtio_blk() -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        bars[0] = Bar::io(VIRTIO_BLK_BAR0_SIZE);
        bars[1] = Bar::memory32(VIRTIO_BLK_BAR1_SIZE);
        bars[4] = Bar::memory32(VIRTIO_BLK_BAR4_SIZE);
        let caps = virtio_caps::boot_media_capability_list();
        let msix = MsixCapability::new(
            VIRTIO_BLK_MSIX_VECTOR_COUNT,
            1,
            VIRTIO_BLK_MSIX_TABLE_OFFSET,
            VIRTIO_BLK_MSIX_PBA_OFFSET,
        );
        let mut cap_bytes = caps.cap_bytes;
        cap_bytes.extend(
            msix.to_bytes(0)
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(VIRTIO_BLK_MSIX_CAP_OFFSET) + i as u16, byte)),
        );
        Self {
            bdf: VIRTIO_BLK_BDF,
            vendor_device: (u32::from(VIRTIO_BLK_DEVICE_ID) << 16)
                | u32::from(VIRTIO_BLK_VENDOR_ID),
            revision_class: (VIRTIO_BLK_CLASS_CODE << 8) | u32::from(VIRTIO_BLK_REVISION),
            subsystem_ids: (u32::from(VIRTIO_BLK_SUBSYSTEM_ID) << 16)
                | u32::from(VIRTIO_BLK_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: caps.cap_ptr,
            cap_bytes,
        }
    }

    /// Modern-only virtio network endpoint at `00:04.0`.
    fn virtio_net() -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        bars[1] = Bar::memory32(VIRTIO_NET_BAR1_SIZE);
        bars[4] = Bar::memory32(VIRTIO_NET_BAR4_SIZE);
        let caps = virtio_caps::capability_list(VIRTIO_NET_MSIX_CAP_OFFSET);
        let msix = MsixCapability::new(
            VIRTIO_NET_MSIX_VECTOR_COUNT,
            1,
            VIRTIO_NET_MSIX_TABLE_OFFSET,
            VIRTIO_NET_MSIX_PBA_OFFSET,
        );
        let mut cap_bytes = caps.cap_bytes;
        cap_bytes.extend(
            msix.to_bytes(0)
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(VIRTIO_NET_MSIX_CAP_OFFSET) + i as u16, byte)),
        );
        Self {
            bdf: VIRTIO_NET_BDF,
            vendor_device: (u32::from(VIRTIO_NET_DEVICE_ID) << 16)
                | u32::from(VIRTIO_NET_VENDOR_ID),
            revision_class: (VIRTIO_NET_CLASS_CODE << 8) | u32::from(VIRTIO_NET_REVISION),
            subsystem_ids: (u32::from(VIRTIO_NET_SUBSYSTEM_ID) << 16)
                | u32::from(VIRTIO_NET_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: caps.cap_ptr,
            cap_bytes,
        }
    }

    /// Modern-only virtio GPU endpoint at `00:05.0`.
    fn virtio_gpu(host_visible_bar_size: Option<u64>, pci_device_id: u16) -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        bars[1] = Bar::memory32(VIRTIO_GPU_BAR1_SIZE);
        if let Some(size) = host_visible_bar_size {
            let size32 = u32::try_from(size)
                .expect("virtio-gpu host-visible BAR size must currently fit in 32 bits");
            let (bar2, bar3) = Bar::memory64_prefetchable(size32);
            bars[2] = bar2;
            bars[3] = bar3;
        }
        bars[4] = Bar::memory32(VIRTIO_GPU_BAR4_SIZE);
        let caps = if let Some(size) = host_visible_bar_size {
            virtio_caps::capability_list_with_shared_memory(
                VIRTIO_GPU_MSIX_CAP_OFFSET,
                VIRTIO_GPU_SHM_ID_HOST_VISIBLE,
                2,
                size,
            )
        } else {
            virtio_caps::capability_list(VIRTIO_GPU_MSIX_CAP_OFFSET)
        };
        let msix = MsixCapability::new(
            VIRTIO_GPU_MSIX_VECTOR_COUNT,
            1,
            VIRTIO_GPU_MSIX_TABLE_OFFSET,
            VIRTIO_GPU_MSIX_PBA_OFFSET,
        );
        let mut cap_bytes = caps.cap_bytes;
        cap_bytes.extend(
            msix.to_bytes(0)
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(VIRTIO_GPU_MSIX_CAP_OFFSET) + i as u16, byte)),
        );
        Self {
            bdf: VIRTIO_GPU_BDF,
            vendor_device: (u32::from(pci_device_id) << 16) | u32::from(VIRTIO_GPU_VENDOR_ID),
            revision_class: (VIRTIO_GPU_CLASS_CODE << 8) | u32::from(VIRTIO_GPU_REVISION),
            subsystem_ids: (u32::from(VIRTIO_GPU_SUBSYSTEM_ID) << 16)
                | u32::from(VIRTIO_GPU_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: caps.cap_ptr,
            cap_bytes,
        }
    }

    /// Modern-only virtio console endpoint at `00:06.0`.
    fn virtio_console() -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        bars[1] = Bar::memory32(VIRTIO_CONSOLE_BAR1_SIZE);
        bars[4] = Bar::memory32(VIRTIO_CONSOLE_BAR4_SIZE);
        let caps = virtio_caps::capability_list(VIRTIO_CONSOLE_MSIX_CAP_OFFSET);
        let msix = MsixCapability::new(
            VIRTIO_CONSOLE_MSIX_VECTOR_COUNT,
            1,
            VIRTIO_CONSOLE_MSIX_TABLE_OFFSET,
            VIRTIO_CONSOLE_MSIX_PBA_OFFSET,
        );
        let mut cap_bytes = caps.cap_bytes;
        cap_bytes.extend(
            msix.to_bytes(0)
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(VIRTIO_CONSOLE_MSIX_CAP_OFFSET) + i as u16, byte)),
        );
        Self {
            bdf: VIRTIO_CONSOLE_BDF,
            vendor_device: (u32::from(VIRTIO_CONSOLE_DEVICE_ID) << 16)
                | u32::from(VIRTIO_CONSOLE_VENDOR_ID),
            revision_class: (VIRTIO_CONSOLE_CLASS_CODE << 8) | u32::from(VIRTIO_CONSOLE_REVISION),
            subsystem_ids: (u32::from(VIRTIO_CONSOLE_SUBSYSTEM_ID) << 16)
                | u32::from(VIRTIO_CONSOLE_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: caps.cap_ptr,
            cap_bytes,
        }
    }

    fn xhci() -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        let (bar0, bar1) = Bar::memory64(XHCI_BAR0_SIZE);
        bars[0] = bar0;
        bars[1] = bar1;
        let msix = MsixCapability::new(
            XHCI_MSIX_VECTOR_COUNT,
            0,
            XHCI_MSIX_TABLE_OFFSET,
            XHCI_MSIX_PBA_OFFSET,
        );
        let mut cap_bytes: Vec<(u16, u8)> = msix
            .to_bytes(XHCI_PCIE_CAP_OFFSET)
            .into_iter()
            .enumerate()
            .map(|(i, byte)| (u16::from(XHCI_MSIX_CAP_OFFSET) + i as u16, byte))
            .collect();
        cap_bytes.extend(
            XHCI_PCIE_CAP_BYTES
                .into_iter()
                .enumerate()
                .map(|(i, byte)| (u16::from(XHCI_PCIE_CAP_OFFSET) + i as u16, byte)),
        );
        Self {
            bdf: XHCI_BDF,
            vendor_device: (u32::from(XHCI_DEVICE_ID) << 16) | u32::from(XHCI_VENDOR_ID),
            revision_class: (XHCI_CLASS_CODE << 8) | u32::from(XHCI_REVISION),
            subsystem_ids: (u32::from(XHCI_SUBSYSTEM_ID) << 16)
                | u32::from(XHCI_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: XHCI_MSIX_CAP_OFFSET,
            cap_bytes,
        }
    }

    fn mmio_target_of_bar(&self, idx: usize, gpa: u64) -> Option<PcieMmioTargetMru> {
        let bar = self.bars.get(idx)?;
        let (base, size) = match bar.kind {
            BarKind::Memory32 => (bar.assigned_base()?, bar.size()),
            BarKind::Memory64Low => {
                let low = bar.assigned_base()?;
                let high = u64::from(self.bars.get(idx + 1)?.value);
                ((high << 32) | low, bar.size())
            }
            BarKind::Memory64High | BarKind::Io => return None,
        };
        let end = base.checked_add(size)?;
        let offset = gpa.checked_sub(base)?;
        (offset < size).then_some(PcieMmioTargetMru {
            base,
            end,
            target: PcieMmioTarget {
                bdf: self.bdf,
                bar_index: idx,
                offset,
            },
        })
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
            REG_SUBSYSTEM_IDS => self.subsystem_ids,
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
            _ if self.write_capability_dword(reg, value) => {}
            // Identity, class and header registers are read-only; capability
            // bytes are read-only in this model.
            _ => {}
        }
    }

    fn capability_byte(&self, off: u16) -> u8 {
        self.cap_bytes
            .iter()
            .find_map(|&(o, v)| (o == off).then_some(v))
            .unwrap_or(0)
    }

    fn set_capability_byte(&mut self, off: u16, value: u8) {
        if let Some((_, v)) = self.cap_bytes.iter_mut().find(|(o, _)| *o == off) {
            *v = value;
        } else {
            self.cap_bytes.push((off, value));
        }
    }

    /// Handle writes into the MSI-X capability. Most fields are read-only; the
    /// guest may only change Message Control bits 14 (function mask) and 15
    /// (MSI-X enable).
    fn write_capability_dword(&mut self, reg: u16, value: u32) -> bool {
        let Some(cap) = self.msix_capability_offset() else {
            return false;
        };
        let cap_end = cap + MsixCapability::SIZE_BYTES;
        if reg + 4 <= cap || reg >= cap_end {
            return false;
        }

        let control_off = cap + 2;
        let mut requested = u16::from_le_bytes([
            self.capability_byte(control_off),
            self.capability_byte(control_off + 1),
        ]);
        let bytes = value.to_le_bytes();
        for (byte, incoming) in bytes.iter().enumerate() {
            let off = reg + byte as u16;
            if off == control_off {
                requested = (requested & !0x00ff) | u16::from(*incoming);
            } else if off == control_off + 1 {
                requested = (requested & !0xff00) | (u16::from(*incoming) << 8);
            }
        }

        let current = u16::from_le_bytes([
            self.capability_byte(control_off),
            self.capability_byte(control_off + 1),
        ]);
        let next = (current & !0xc000) | (requested & 0xc000);
        let [lo, hi] = next.to_le_bytes();
        self.set_capability_byte(control_off, lo);
        self.set_capability_byte(control_off + 1, hi);
        true
    }

    fn msix_control(&self) -> Option<MsixFunctionControl> {
        let control_off = self.msix_capability_offset()? + 2;
        let control = u16::from_le_bytes([
            self.capability_byte(control_off),
            self.capability_byte(control_off + 1),
        ]);
        Some(MsixFunctionControl {
            enabled: control & 0x8000 != 0,
            function_masked: control & 0x4000 != 0,
        })
    }

    fn msix_capability_offset(&self) -> Option<u16> {
        let mut cap = self.cap_ptr;
        for _ in 0..32 {
            if cap == 0 {
                return None;
            }
            let off = u16::from(cap);
            if self.capability_byte(off) == CAP_ID_MSIX {
                return Some(off);
            }
            cap = self.capability_byte(off + 1);
        }
        None
    }
}

// ---- The ECAM device --------------------------------------------------------

/// The PCIe ECAM config-space model: decodes accesses to the
/// [`PCIE_ECAM`] window and answers for the host bridge (and any future
/// endpoints), returning all-ones for empty slots.
#[derive(Debug, Clone)]
pub struct PcieEcam {
    functions: Vec<Function>,
    mmio_mru: Cell<Option<PcieMmioTargetMru>>,
}

/// A decoded memory-space access into a programmed PCI BAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcieMmioTarget {
    pub bdf: (u8, u8, u8),
    pub bar_index: usize,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PcieMmioTargetMru {
    base: u64,
    end: u64,
    target: PcieMmioTarget,
}

impl PcieMmioTargetMru {
    fn target_for(self, gpa: u64) -> Option<PcieMmioTarget> {
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

    fn function_at(&self, bdf: (u8, u8, u8)) -> Option<&Function> {
        self.functions.iter().find(|f| f.bdf == bdf)
    }

    fn function_at_mut(&mut self, bdf: (u8, u8, u8)) -> Option<&mut Function> {
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
        extract(dword, addr.reg, size)
    }

    /// Write `size` (1, 2 or 4) bytes of config space at `ecam_offset`. Writes to
    /// empty slots are dropped. A function performs a read-modify-write so a
    /// sub-dword write only touches the addressed bytes (the command register and
    /// BARs are word/dword-aligned in practice).
    pub fn cfg_write(&mut self, ecam_offset: u64, size: u8, value: u64) {
        self.mmio_mru.set(None);
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
        let low = func.bars[2].assigned_base()?;
        let high = u64::from(func.bars[3].value);
        Some((high << 32) | low)
    }

    pub fn virtio_gpu_host_visible_bar_size(&self) -> Option<u64> {
        self.function_at(VIRTIO_GPU_BDF)
            .map(|func| func.bars[2].size())
            .filter(|size| *size != 0)
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
    use std::sync::{Mutex, OnceLock};

    /// Build a raw ECAM offset for a (bus, dev, fn, reg) tuple, the way the run
    /// loop derives it from a guest fault address minus the window base.
    fn ecam_offset(bus: u8, dev: u8, func: u8, reg: u16) -> u64 {
        (u64::from(bus) << SHIFT_BUS)
            | (u64::from(dev) << SHIFT_DEV)
            | (u64::from(func) << SHIFT_FN)
            | u64::from(reg)
    }

    fn bdf_ecam_offset(bdf: (u8, u8, u8), reg: u16) -> u64 {
        ecam_offset(bdf.0, bdf.1, bdf.2, reg)
    }

    fn read_config_bytes(ecam: &PcieEcam, bdf: (u8, u8, u8), len: usize) -> Vec<u8> {
        (0..len)
            .map(|reg| {
                u8::try_from(ecam.cfg_read(bdf_ecam_offset(bdf, reg as u16), 1))
                    .expect("single-byte config read fits in u8")
            })
            .collect()
    }

    fn find_vendor_cfg_type(ecam: &PcieEcam, bdf: (u8, u8, u8), cfg_type: u8) -> Option<u16> {
        let mut cap = ecam.cfg_read(bdf_ecam_offset(bdf, REG_CAP_PTR), 1) as u8;
        for _ in 0..32 {
            if cap == 0 {
                return None;
            }
            let cap_id = ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap)), 1) as u8;
            let next = ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap) + 1), 1) as u8;
            if cap_id == 0x09
                && ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap) + 3), 1) as u8 == cfg_type
            {
                return Some(u16::from(cap));
            }
            cap = next;
        }
        None
    }

    fn cap_chain_contains_vendor_cfg_type(
        ecam: &PcieEcam,
        bdf: (u8, u8, u8),
        cfg_type: u8,
    ) -> bool {
        find_vendor_cfg_type(ecam, bdf, cfg_type).is_some()
    }

    fn with_hostmem_mib_env<R>(value: &str, f: impl FnOnce() -> R) -> R {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let previous = std::env::var_os("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB");
        std::env::set_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", value);
        let result = f();
        match previous {
            Some(previous) => std::env::set_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", previous),
            None => std::env::remove_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB"),
        }
        result
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
        assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 4), NO_DEVICE);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 2), 0xFFFF);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 1), 0xFF);
        // A different function of device 0 is also empty.
        assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 1, 0x00), 4), NO_DEVICE);
        // A non-zero bus is empty.
        assert_eq!(ecam.cfg_read(ecam_offset(1, 0, 0, 0x00), 4), NO_DEVICE);
    }

    #[test]
    fn boot_media_config_space_bytes_stay_byte_identical() {
        let ecam = PcieEcam::new();
        let mut expected = [0u8; 0x100];
        expected[0x00..0x04].copy_from_slice(&[0xf4, 0x1a, 0x01, 0x10]);
        expected[0x04..0x08].copy_from_slice(&[0x00, 0x00, 0x10, 0x00]);
        expected[0x08..0x0c].copy_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        expected[0x2c..0x30].copy_from_slice(&[0xf4, 0x1a, 0x02, 0x00]);
        expected[0x34] = 0x40;
        expected[0x40..0x50].copy_from_slice(&[
            0x09, 0x50, 0x10, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
            0x00, 0x00,
        ]);
        expected[0x50..0x64].copy_from_slice(&[
            0x09, 0x64, 0x14, 0x02, 0x04, 0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x10,
            0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
        ]);
        expected[0x64..0x74].copy_from_slice(&[
            0x09, 0x74, 0x10, 0x03, 0x04, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10,
            0x00, 0x00,
        ]);
        expected[0x74..0x84].copy_from_slice(&[
            0x09, 0x84, 0x10, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x10,
            0x00, 0x00,
        ]);
        expected[0x84..0x90].copy_from_slice(&[
            0x11, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00,
        ]);

        assert_eq!(
            read_config_bytes(&ecam, VIRTIO_BLK_BDF, expected.len()),
            expected
        );
    }

    #[test]
    fn boot_media_endpoint_reports_qemu_oracle_identity_at_00_03_0() {
        let ecam = PcieEcam::new();
        let (bus, dev, func) = VIRTIO_BLK_BDF;

        let vd = ecam.cfg_read(ecam_offset(bus, dev, func, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xFFFF, u64::from(VIRTIO_BLK_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xFFFF, u64::from(VIRTIO_BLK_DEVICE_ID));

        let rc = ecam.cfg_read(ecam_offset(bus, dev, func, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(VIRTIO_BLK_CLASS_CODE));
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, 0x0e), 1),
            u64::from(HEADER_TYPE_ENDPOINT)
        );
    }

    #[test]
    fn boot_media_endpoint_reports_qemu_oracle_subsystem_id() {
        let ecam = PcieEcam::new();
        let (bus, dev, func) = VIRTIO_BLK_BDF;

        let subsystem = ecam.cfg_read(ecam_offset(bus, dev, func, REG_SUBSYSTEM_IDS), 4);
        assert_eq!(
            subsystem & 0xFFFF,
            u64::from(VIRTIO_BLK_SUBSYSTEM_VENDOR_ID)
        );
        assert_eq!(
            (subsystem >> 16) & 0xFFFF,
            u64::from(VIRTIO_BLK_SUBSYSTEM_ID)
        );
    }

    #[test]
    fn boot_media_given_bars_when_sizing_then_matches_qemu_oracle_shape() {
        let mut ecam = PcieEcam::new();
        let (bus, dev, func) = VIRTIO_BLK_BDF;

        // Given: QEMU's virtio-blk-pci exposes BAR0 as 0x80 bytes of PIO.
        let bar0 = ecam_offset(bus, dev, func, REG_BAR0);
        ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
        let bar0_readback = ecam.cfg_read(bar0, 4) as u32;
        let bar0_size = (!(bar0_readback & !0x3)).wrapping_add(1);
        assert_eq!(bar0_readback & 0x1, 0x1, "BAR0 must be an I/O BAR");
        assert_eq!(bar0_size, 0x80);

        // Given: BAR1 is the 32-bit memory aperture used for MSI-X.
        let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
        ecam.cfg_write(bar1, 4, 0xFFFF_FFFF);
        let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
        assert_eq!(bar1_readback & 0xF, 0, "BAR1 must be 32-bit memory");
        assert_eq!((!(bar1_readback & !0xF)).wrapping_add(1), 0x1000);

        // Then: BAR4 is the modern virtio MMIO transport block, sized 0x4000.
        let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
        ecam.cfg_write(bar4, 4, 0xFFFF_FFFF);
        let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
        assert_eq!(bar4_readback & 0xF, 0, "BAR4 must be 32-bit memory");
        assert_eq!((!(bar4_readback & !0xF)).wrapping_add(1), 0x4000);
    }

    #[test]
    fn boot_media_given_bars_when_command_bits_change_then_pio_and_mmio_decode_separately() {
        let mut ecam = PcieEcam::new();
        let (bus, dev, func) = VIRTIO_BLK_BDF;
        let bar0 = ecam_offset(bus, dev, func, REG_BAR0);
        let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
        let cmd = ecam_offset(bus, dev, func, REG_COMMAND_STATUS);
        let pio_base = 0xC000;
        let mmio_base = machine::PCIE_MMIO_32.base + 0x1_0000;

        // Given: firmware programmed both BAR0 PIO and BAR4 MMIO bases.
        ecam.cfg_write(bar0, 4, pio_base);
        ecam.cfg_write(bar4, 4, mmio_base);
        assert_eq!(ecam.pio_target(pio_base), None);
        assert_eq!(ecam.mmio_target(mmio_base), None);

        // When: only I/O space is enabled, only BAR0 decodes.
        ecam.cfg_write(cmd, 2, u64::from(CMD_IO_SPACE));
        assert_eq!(
            ecam.pio_target(pio_base),
            Some(PciePioTarget {
                bdf: VIRTIO_BLK_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        assert_eq!(ecam.mmio_target(mmio_base), None);

        // When: only memory space is enabled, only BAR4 decodes.
        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
        assert_eq!(ecam.pio_target(pio_base), None);
        assert_eq!(
            ecam.mmio_target(mmio_base),
            Some(PcieMmioTarget {
                bdf: VIRTIO_BLK_BDF,
                bar_index: 4,
                offset: 0,
            })
        );
    }

    #[test]
    fn virtio_net_endpoint_is_absent_by_default_and_gated_on() {
        let ecam = PcieEcam::new();
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_VENDOR_DEVICE), 4),
            NO_DEVICE
        );

        let ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            ..PcieEcamConfig::default()
        });
        let vd = ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xffff, u64::from(VIRTIO_NET_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xffff, u64::from(VIRTIO_NET_DEVICE_ID));
        let rc = ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(VIRTIO_NET_CLASS_CODE));
        assert_eq!(rc & 0xff, u64::from(VIRTIO_NET_REVISION));
        let subsystem = ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_SUBSYSTEM_IDS), 4);
        assert_eq!(
            subsystem & 0xffff,
            u64::from(VIRTIO_NET_SUBSYSTEM_VENDOR_ID)
        );
        assert_eq!(
            (subsystem >> 16) & 0xffff,
            u64::from(VIRTIO_NET_SUBSYSTEM_ID)
        );
    }

    #[test]
    fn virtio_net_modern_bars_and_capabilities_match_stage1_shape() {
        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            ..PcieEcamConfig::default()
        });
        let (bus, dev, func) = VIRTIO_NET_BDF;
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, REG_CAP_PTR), 1),
            0x40
        );

        let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
        ecam.cfg_write(bar1, 4, 0xffff_ffff);
        let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
        assert_eq!(bar1_readback & 0xf, 0);
        assert_eq!(
            (!(bar1_readback & !0xf)).wrapping_add(1),
            VIRTIO_NET_BAR1_SIZE
        );

        let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
        ecam.cfg_write(bar4, 4, 0xffff_ffff);
        let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
        assert_eq!(bar4_readback & 0xf, 0);
        assert_eq!(
            (!(bar4_readback & !0xf)).wrapping_add(1),
            VIRTIO_NET_BAR4_SIZE
        );

        let msix = u16::from(VIRTIO_NET_MSIX_CAP_OFFSET);
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix), 1),
            u64::from(CAP_ID_MSIX)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 2), 2),
            u64::from(VIRTIO_NET_MSIX_VECTOR_COUNT - 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 4), 4),
            u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET | 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 8), 4),
            u64::from(VIRTIO_NET_MSIX_PBA_OFFSET | 1)
        );
    }

    #[test]
    fn virtio_net_bar1_and_bar4_decode_after_command_enable() {
        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            ..PcieEcamConfig::default()
        });
        let (bus, dev, func) = VIRTIO_NET_BDF;
        let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
        let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
        let cmd = ecam_offset(bus, dev, func, REG_COMMAND_STATUS);
        let bar1_base = machine::PCIE_MMIO_32.base + 0x4_0000;
        let bar4_base = machine::PCIE_MMIO_32.base + 0x5_0000;

        ecam.cfg_write(bar1, 4, bar1_base);
        ecam.cfg_write(bar4, 4, bar4_base);
        assert_eq!(ecam.mmio_target(bar1_base), None);
        assert_eq!(ecam.mmio_target(bar4_base), None);

        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
        assert_eq!(
            ecam.mmio_target(bar1_base),
            Some(PcieMmioTarget {
                bdf: VIRTIO_NET_BDF,
                bar_index: 1,
                offset: 0,
            })
        );
        assert_eq!(
            ecam.mmio_target(bar4_base),
            Some(PcieMmioTarget {
                bdf: VIRTIO_NET_BDF,
                bar_index: 4,
                offset: 0,
            })
        );
    }

    #[test]
    fn virtio_gpu_endpoint_is_absent_by_default_and_gated_on_without_regressing_net_or_blk() {
        let baseline = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            ..PcieEcamConfig::default()
        });
        assert_eq!(
            baseline.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE), 4),
            NO_DEVICE
        );
        let baseline_blk = read_config_bytes(&baseline, VIRTIO_BLK_BDF, 256);
        let baseline_net = read_config_bytes(&baseline, VIRTIO_NET_BDF, 256);

        let ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            virtio_gpu_present: true,
            ..PcieEcamConfig::default()
        });
        assert_eq!(read_config_bytes(&ecam, VIRTIO_BLK_BDF, 256), baseline_blk);
        assert_eq!(read_config_bytes(&ecam, VIRTIO_NET_BDF, 256), baseline_net);

        let vd = ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xffff, u64::from(VIRTIO_GPU_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xffff, u64::from(VIRTIO_GPU_DEVICE_ID));
        let rc = ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(VIRTIO_GPU_CLASS_CODE));
        assert_eq!(rc & 0xff, u64::from(VIRTIO_GPU_REVISION));
        let subsystem = ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_SUBSYSTEM_IDS), 4);
        assert_eq!(
            subsystem & 0xffff,
            u64::from(VIRTIO_GPU_SUBSYSTEM_VENDOR_ID)
        );
        assert_eq!(
            (subsystem >> 16) & 0xffff,
            u64::from(VIRTIO_GPU_SUBSYSTEM_ID)
        );
    }

    #[test]
    fn virtio_console_endpoint_is_absent_by_default_and_gated_on_without_regressing_other_virtio() {
        let baseline = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            virtio_gpu_present: true,
            ..PcieEcamConfig::default()
        });
        assert_eq!(
            baseline.cfg_read(bdf_ecam_offset(VIRTIO_CONSOLE_BDF, REG_VENDOR_DEVICE), 4),
            NO_DEVICE
        );
        let baseline_blk = read_config_bytes(&baseline, VIRTIO_BLK_BDF, 256);
        let baseline_net = read_config_bytes(&baseline, VIRTIO_NET_BDF, 256);
        let baseline_gpu = read_config_bytes(&baseline, VIRTIO_GPU_BDF, 256);

        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            virtio_gpu_present: true,
            virtio_console_present: true,
            ..PcieEcamConfig::default()
        });
        assert_eq!(read_config_bytes(&ecam, VIRTIO_BLK_BDF, 256), baseline_blk);
        assert_eq!(read_config_bytes(&ecam, VIRTIO_NET_BDF, 256), baseline_net);
        assert_eq!(read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256), baseline_gpu);

        let (bus, dev, func) = VIRTIO_CONSOLE_BDF;
        let vd = ecam.cfg_read(ecam_offset(bus, dev, func, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xffff, u64::from(VIRTIO_CONSOLE_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xffff, u64::from(VIRTIO_CONSOLE_DEVICE_ID));
        let rc = ecam.cfg_read(ecam_offset(bus, dev, func, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(VIRTIO_CONSOLE_CLASS_CODE));
        assert_eq!(rc & 0xff, u64::from(VIRTIO_CONSOLE_REVISION));
        let subsystem = ecam.cfg_read(ecam_offset(bus, dev, func, REG_SUBSYSTEM_IDS), 4);
        assert_eq!(
            subsystem & 0xffff,
            u64::from(VIRTIO_CONSOLE_SUBSYSTEM_VENDOR_ID)
        );
        assert_eq!(
            (subsystem >> 16) & 0xffff,
            u64::from(VIRTIO_CONSOLE_SUBSYSTEM_ID)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, REG_CAP_PTR), 1),
            0x40
        );

        let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
        ecam.cfg_write(bar1, 4, 0xffff_ffff);
        let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
        assert_eq!(bar1_readback & 0xf, 0);
        assert_eq!(
            (!(bar1_readback & !0xf)).wrapping_add(1),
            VIRTIO_CONSOLE_BAR1_SIZE
        );

        let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
        ecam.cfg_write(bar4, 4, 0xffff_ffff);
        let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
        assert_eq!(bar4_readback & 0xf, 0);
        assert_eq!(
            (!(bar4_readback & !0xf)).wrapping_add(1),
            VIRTIO_CONSOLE_BAR4_SIZE
        );

        let msix = u16::from(VIRTIO_CONSOLE_MSIX_CAP_OFFSET);
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix), 1),
            u64::from(CAP_ID_MSIX)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 2), 2),
            u64::from(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT - 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 4), 4),
            u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET | 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 8), 4),
            u64::from(VIRTIO_CONSOLE_MSIX_PBA_OFFSET | 1)
        );
        assert!(cap_chain_contains_vendor_cfg_type(
            &ecam,
            VIRTIO_CONSOLE_BDF,
            1
        ));
        assert!(cap_chain_contains_vendor_cfg_type(
            &ecam,
            VIRTIO_CONSOLE_BDF,
            2
        ));
        assert!(cap_chain_contains_vendor_cfg_type(
            &ecam,
            VIRTIO_CONSOLE_BDF,
            3
        ));
        assert!(cap_chain_contains_vendor_cfg_type(
            &ecam,
            VIRTIO_CONSOLE_BDF,
            4
        ));
    }

    #[test]
    fn virtio_gpu_pci_device_id_defaults_and_can_be_overridden_without_changing_device_shape() {
        let default_ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            virtio_gpu_present: true,
            ..PcieEcamConfig::default()
        });
        let override_ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_net_present: true,
            virtio_gpu_present: true,
            virtio_gpu_pci_device_id: 0x10f7,
            ..PcieEcamConfig::default()
        });

        assert_eq!(
            default_ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE + 2), 2),
            u64::from(VIRTIO_GPU_DEVICE_ID)
        );
        assert_eq!(
            override_ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE + 2), 2),
            0x10f7
        );

        let mut default_gpu = read_config_bytes(&default_ecam, VIRTIO_GPU_BDF, 256);
        let mut override_gpu = read_config_bytes(&override_ecam, VIRTIO_GPU_BDF, 256);
        assert_eq!(
            &default_gpu[0..2],
            &override_gpu[0..2],
            "virtio-gpu vendor id must not change"
        );
        assert_ne!(&default_gpu[2..4], &override_gpu[2..4]);
        default_gpu[2..4].copy_from_slice(&[0, 0]);
        override_gpu[2..4].copy_from_slice(&[0, 0]);
        assert_eq!(
            default_gpu, override_gpu,
            "only the virtio-gpu PCI device-id field may differ"
        );

        for bdf in [NVME_BDF, XHCI_BDF, VIRTIO_BLK_BDF, VIRTIO_NET_BDF] {
            assert_eq!(
                read_config_bytes(&default_ecam, bdf, 256),
                read_config_bytes(&override_ecam, bdf, 256),
                "non-GPU PCI function {bdf:?} changed"
            );
        }
    }

    #[test]
    fn virtio_gpu_modern_bars_and_capabilities_match_stage_g1_shape() {
        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_gpu_present: true,
            ..PcieEcamConfig::default()
        });
        let (bus, dev, func) = VIRTIO_GPU_BDF;
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, REG_CAP_PTR), 1),
            0x40
        );

        let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
        ecam.cfg_write(bar1, 4, 0xffff_ffff);
        let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
        assert_eq!(
            (!(bar1_readback & !0xf)).wrapping_add(1),
            VIRTIO_GPU_BAR1_SIZE
        );

        let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
        ecam.cfg_write(bar4, 4, 0xffff_ffff);
        let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
        assert_eq!(
            (!(bar4_readback & !0xf)).wrapping_add(1),
            VIRTIO_GPU_BAR4_SIZE
        );

        let msix = u16::from(VIRTIO_GPU_MSIX_CAP_OFFSET);
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix), 1),
            u64::from(CAP_ID_MSIX)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 2), 2),
            u64::from(VIRTIO_GPU_MSIX_VECTOR_COUNT - 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 4), 4),
            u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET | 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(bus, dev, func, msix + 8), 4),
            u64::from(VIRTIO_GPU_MSIX_PBA_OFFSET | 1)
        );

        let bar1_base = machine::PCIE_MMIO_32.base + 0x6_0000;
        let bar4_base = machine::PCIE_MMIO_32.base + 0x7_0000;
        ecam.cfg_write(bar1, 4, bar1_base);
        ecam.cfg_write(bar4, 4, bar4_base);
        ecam.cfg_write(
            ecam_offset(bus, dev, func, REG_COMMAND_STATUS),
            2,
            u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
        );
        assert_eq!(
            ecam.mmio_target(bar1_base),
            Some(PcieMmioTarget {
                bdf: VIRTIO_GPU_BDF,
                bar_index: 1,
                offset: 0,
            })
        );
        assert_eq!(
            ecam.mmio_target(bar4_base),
            Some(PcieMmioTarget {
                bdf: VIRTIO_GPU_BDF,
                bar_index: 4,
                offset: 0,
            })
        );
    }

    #[test]
    fn virtio_gpu_2d_config_has_no_host_visible_bar_or_shared_memory_capability() {
        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_gpu_present: true,
            virtio_gpu_3d_enabled: false,
            ..PcieEcamConfig::default()
        });
        let before = read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256);
        let bar2 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 2);
        ecam.cfg_write(bar2, 4, 0xffff_ffff);
        assert_eq!(ecam.cfg_read(bar2, 4), 0);
        assert_eq!(ecam.virtio_gpu_host_visible_bar_size(), None);
        assert!(!cap_chain_contains_vendor_cfg_type(
            &ecam,
            VIRTIO_GPU_BDF,
            8
        ));

        let after = read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256);
        assert_eq!(
            before, after,
            "BAR2 sizing on the 2D shape must not mutate config bytes"
        );
    }

    #[test]
    fn virtio_gpu_3d_exposes_prefetchable_bar2_and_host_visible_shm_capability() {
        with_hostmem_mib_env("1024", || {
            let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
                virtio_gpu_present: true,
                virtio_gpu_3d_enabled: true,
                ..PcieEcamConfig::default()
            });
            let bar2 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 2);
            let bar3 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 3);
            ecam.cfg_write(bar2, 4, 0xffff_ffff);
            ecam.cfg_write(bar3, 4, 0xffff_ffff);
            assert_eq!(ecam.cfg_read(bar2, 4), 0xc000_000c);
            assert_eq!(ecam.cfg_read(bar3, 4), 0xffff_ffff);
            assert_eq!(
                ecam.virtio_gpu_host_visible_bar_size(),
                Some(VIRTIO_GPU_HOSTMEM_DEFAULT_SIZE)
            );

            let cap = find_vendor_cfg_type(&ecam, VIRTIO_GPU_BDF, 8).expect("shared-memory cap");
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 2), 1),
                24
            );
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 4), 1),
                2
            );
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 5), 1),
                u64::from(VIRTIO_GPU_SHM_ID_HOST_VISIBLE)
            );
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 8), 4),
                0
            );
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 12), 4),
                VIRTIO_GPU_HOSTMEM_DEFAULT_SIZE & 0xffff_ffff
            );
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 16), 4),
                0
            );
            assert_eq!(
                ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 20), 4),
                0
            );
        });
    }

    #[test]
    fn virtio_gpu_3d_with_zero_hostmem_omits_bar2_and_shared_memory_capability() {
        with_hostmem_mib_env("0", || {
            assert_eq!(parse_virtio_gpu_hostmem_size(), 0);
            let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
                virtio_gpu_present: true,
                virtio_gpu_3d_enabled: true,
                ..PcieEcamConfig::default()
            });
            let before = read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256);
            let bar2 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 2);
            let bar3 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 3);
            ecam.cfg_write(bar2, 4, 0xffff_ffff);
            ecam.cfg_write(bar3, 4, 0xffff_ffff);
            assert_eq!(ecam.cfg_read(bar2, 4), 0);
            assert_eq!(ecam.cfg_read(bar3, 4), 0);
            assert_eq!(ecam.virtio_gpu_host_visible_bar_size(), None);
            assert!(!cap_chain_contains_vendor_cfg_type(
                &ecam,
                VIRTIO_GPU_BDF,
                8
            ));
            assert_eq!(
                before,
                read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256),
                "zero-hostmem BAR2/BAR3 sizing must not mutate config bytes"
            );
        });
    }

    #[test]
    fn bar_decode_ignores_addresses_below_programmed_base() {
        let mut ecam = PcieEcam::new();
        let (bus, dev, func) = VIRTIO_BLK_BDF;
        let pio_bar0 = ecam_offset(bus, dev, func, REG_BAR0);
        let pio_cmd = ecam_offset(bus, dev, func, REG_COMMAND_STATUS);
        let pio_base = 0xc000;

        ecam.cfg_write(pio_bar0, 4, pio_base);
        ecam.cfg_write(pio_cmd, 2, u64::from(CMD_IO_SPACE));

        assert_eq!(ecam.pio_target(pio_base - 1), None);
        assert_eq!(
            ecam.pio_target(pio_base),
            Some(PciePioTarget {
                bdf: VIRTIO_BLK_BDF,
                bar_index: 0,
                offset: 0,
            })
        );

        let xhci_bar0 = ecam_offset(0, 2, 0, REG_BAR0);
        let xhci_bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
        let xhci_cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
        let mmio_base = machine::PCIE_MMIO_32.base + 0x2_0000;

        ecam.cfg_write(xhci_bar0, 4, mmio_base);
        ecam.cfg_write(xhci_bar1, 4, 0);
        ecam.cfg_write(xhci_cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

        assert_eq!(ecam.mmio_target(mmio_base - 1), None);
        assert_eq!(
            ecam.mmio_target(mmio_base),
            Some(PcieMmioTarget {
                bdf: XHCI_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
    }

    #[test]
    fn qemu_xhci_endpoint_reports_oracle_identity_at_00_02_0() {
        let ecam = PcieEcam::new();

        let vd = ecam.cfg_read(ecam_offset(0, 2, 0, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xFFFF, u64::from(XHCI_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xFFFF, u64::from(XHCI_DEVICE_ID));

        let rc = ecam.cfg_read(ecam_offset(0, 2, 0, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(XHCI_CLASS_CODE));
        assert_eq!(rc & 0xFF, u64::from(XHCI_REVISION));

        let subsystem = ecam.cfg_read(ecam_offset(0, 2, 0, REG_SUBSYSTEM_IDS), 4);
        assert_eq!(subsystem & 0xFFFF, u64::from(XHCI_SUBSYSTEM_VENDOR_ID));
        assert_eq!((subsystem >> 16) & 0xFFFF, u64::from(XHCI_SUBSYSTEM_ID));
    }

    #[test]
    fn qemu_xhci_exposes_msix_and_pcie_capabilities() {
        let ecam = PcieEcam::new();
        let status = ecam.cfg_read(ecam_offset(0, 2, 0, REG_COMMAND_STATUS), 4) >> 16;
        assert_ne!(status & u64::from(STATUS_CAP_LIST), 0);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, REG_CAP_PTR), 1),
            u64::from(XHCI_MSIX_CAP_OFFSET)
        );

        let msix = u16::from(XHCI_MSIX_CAP_OFFSET);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, msix), 1),
            u64::from(CAP_ID_MSIX)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, msix + 1), 1),
            u64::from(XHCI_PCIE_CAP_OFFSET)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, msix + 2), 2),
            u64::from(XHCI_MSIX_VECTOR_COUNT - 1)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, msix + 4), 4),
            u64::from(XHCI_MSIX_TABLE_OFFSET)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, msix + 8), 4),
            u64::from(XHCI_MSIX_PBA_OFFSET)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 2, 0, u16::from(XHCI_PCIE_CAP_OFFSET)), 1),
            0x10
        );
    }

    #[test]
    fn qemu_xhci_bar0_is_64bit_16k_memory_bar() {
        let mut ecam = PcieEcam::new();
        let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
        let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);

        ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
        ecam.cfg_write(bar1, 4, 0xFFFF_FFFF);

        assert_eq!(ecam.cfg_read(bar0, 4), 0xffff_c004);
        assert_eq!(ecam.cfg_read(bar1, 4), 0xffff_ffff);
    }

    #[test]
    fn qemu_xhci_64bit_bar_decodes_low_mmio_after_command_enable() {
        let mut ecam = PcieEcam::new();
        let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
        let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
        let cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
        let base = machine::PCIE_MMIO_32.base + 0x2_0000;

        ecam.cfg_write(bar0, 4, base);
        ecam.cfg_write(bar1, 4, 0);
        assert_eq!(ecam.mmio_target(base), None);

        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
        assert_eq!(
            ecam.mmio_target(base),
            Some(PcieMmioTarget {
                bdf: XHCI_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        assert_eq!(
            ecam.mmio_target(base + 0x3fff).map(|target| target.offset),
            Some(0x3fff)
        );
        assert_eq!(ecam.mmio_target(base + u64::from(XHCI_BAR0_SIZE)), None);
    }

    #[test]
    fn qemu_xhci_64bit_bar_decodes_high_mmio_after_command_enable() {
        let mut ecam = PcieEcam::new();
        let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
        let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
        let cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
        let base = machine::PCIE_MMIO_64.base + 0x2_0000;

        ecam.cfg_write(bar0, 4, base & 0xffff_ffff);
        ecam.cfg_write(bar1, 4, base >> 32);
        assert_eq!(ecam.mmio_target(base), None);

        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
        assert_eq!(ecam.mmio_target(base - 1), None);
        assert_eq!(
            ecam.mmio_target(base),
            Some(PcieMmioTarget {
                bdf: XHCI_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        assert_eq!(
            ecam.mmio_target(base + 0x3fff).map(|target| target.offset),
            Some(0x3fff)
        );
        assert_eq!(ecam.mmio_target(base + u64::from(XHCI_BAR0_SIZE)), None);
    }

    #[test]
    fn writes_to_empty_slots_are_dropped() {
        let mut ecam = PcieEcam::new();
        ecam.cfg_write(ecam_offset(0, 4, 0, REG_COMMAND_STATUS), 2, 0x7);
        // Still empty.
        assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 4), NO_DEVICE);
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
    fn nvme_endpoint_reports_storage_class_and_bar0() {
        let ecam = PcieEcam::new();
        let vd = ecam.cfg_read(ecam_offset(0, 1, 0, REG_VENDOR_DEVICE), 4);
        assert_eq!(vd & 0xFFFF, u64::from(NVME_VENDOR_ID));
        assert_eq!((vd >> 16) & 0xFFFF, u64::from(NVME_DEVICE_ID));

        let rc = ecam.cfg_read(ecam_offset(0, 1, 0, REG_REVISION_CLASS), 4);
        assert_eq!(rc >> 8, u64::from(NVME_CLASS_CODE));
        assert_eq!(rc & 0xFF, u64::from(NVME_REVISION));

        // BAR0 is a 64-bit memory BAR: unprogrammed it reads back only its
        // hardwired type bits (bit 2 = 64-bit), matching QEMU's NVMe endpoint.
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, REG_BAR0), 4), 0x4);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, REG_BAR0 + 4), 4), 0);
    }

    #[test]
    fn nvme_endpoint_exposes_msix_capability() {
        let ecam = PcieEcam::new();
        let status = ecam.cfg_read(ecam_offset(0, 1, 0, REG_COMMAND_STATUS), 4) >> 16;
        assert_ne!(
            status & u64::from(STATUS_CAP_LIST),
            0,
            "NVMe endpoint must advertise a PCI capability list"
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, REG_CAP_PTR), 1),
            u64::from(NVME_MSIX_CAP_OFFSET)
        );

        let cap = u16::from(NVME_MSIX_CAP_OFFSET);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, cap), 1),
            u64::from(CAP_ID_MSIX)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, cap + 1), 1),
            u64::from(NVME_PM_CAP_OFFSET),
            "MSI-X capability must chain to the Power Management capability"
        );
        // Power Management capability (ID 0x01) chains onward to PCI Express.
        let pm = u16::from(NVME_PM_CAP_OFFSET);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, pm), 1), 0x01);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, pm + 1), 1),
            u64::from(NVME_PCIE_CAP_OFFSET),
            "Power Management capability must chain to PCI Express"
        );
        // The PCI Express capability (ID 0x10) that EDK2's NvmExpressDxe needs
        // terminates the list.
        let pcie = u16::from(NVME_PCIE_CAP_OFFSET);
        assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, pcie), 1), 0x10);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, pcie + 1), 1),
            0,
            "PCI Express capability terminates the list"
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, cap + 2), 2),
            u64::from(NVME_MSIX_VECTOR_COUNT - 1),
            "MSI-X table-size field is encoded as count - 1"
        );
        assert_eq!(
            NVME_MSIX_VECTOR_COUNT, 9,
            "NVMe advertises one admin vector plus eight I/O vectors"
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, cap + 4), 4),
            u64::from(NVME_MSIX_TABLE_OFFSET)
        );
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, cap + 8), 4),
            u64::from(NVME_MSIX_PBA_OFFSET)
        );
        assert!(
            NVME_MSIX_TABLE_OFFSET
                + u32::from(NVME_MSIX_VECTOR_COUNT) * MsixCapability::ENTRY_BYTES
                <= NVME_BAR0_SIZE
        );
        assert!(NVME_MSIX_PBA_OFFSET + 8 <= NVME_BAR0_SIZE);
    }

    #[test]
    fn nvme_msix_enable_and_function_mask_bits_are_writable() {
        let mut ecam = PcieEcam::new();
        let control = u16::from(NVME_MSIX_CAP_OFFSET) + 2;

        assert_eq!(ecam.nvme_msix_control(), MsixFunctionControl::default());

        // The table-size bits are read-only; only function-mask and enable move.
        ecam.cfg_write(ecam_offset(0, 1, 0, control), 2, 0xffff);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, control), 2),
            u64::from(0xc000 | (NVME_MSIX_VECTOR_COUNT - 1))
        );
        assert_eq!(
            ecam.nvme_msix_control(),
            MsixFunctionControl {
                enabled: true,
                function_masked: true,
            }
        );

        ecam.cfg_write(ecam_offset(0, 1, 0, control + 1), 1, 0x00);
        assert_eq!(
            ecam.cfg_read(ecam_offset(0, 1, 0, control), 2),
            u64::from(NVME_MSIX_VECTOR_COUNT - 1),
            "sub-byte writes clear the writable MSI-X control bits"
        );
        assert_eq!(ecam.nvme_msix_control(), MsixFunctionControl::default());
    }

    #[test]
    fn nvme_command_enables_bar0_mmio_decode() {
        let mut ecam = PcieEcam::new();
        let bar0 = ecam_offset(0, 1, 0, REG_BAR0);
        let bar1 = ecam_offset(0, 1, 0, REG_BAR0 + 4);
        let cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);

        ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
        let readback = ecam.cfg_read(bar0, 4) as u32;
        // Bit 2 of a 64-bit memory BAR is the hardwired type indicator; mask
        // the low 4 bits before computing the aperture size.
        let size = (!(readback & !0xF)).wrapping_add(1);
        assert_eq!(size, NVME_BAR0_SIZE);
        assert_eq!(readback & 0x6, 0x4, "NVMe BAR0 must advertise 64-bit type");

        let base = machine::PCIE_MMIO_32.base as u32;
        ecam.cfg_write(bar0, 4, u64::from(base));
        ecam.cfg_write(bar1, 4, 0);
        assert_eq!(ecam.cfg_read(bar0, 4) & !0xF, u64::from(base));
        assert!(ecam.nvme_endpoint_state().bar0_assigned);
        assert!(!ecam.nvme_endpoint_state().command_memory_enabled);
        assert!(!ecam.nvme_endpoint_state().command_bus_master_enabled);
        assert_eq!(ecam.mmio_target(machine::PCIE_MMIO_32.base), None);

        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
        assert!(ecam.nvme_endpoint_state().command_memory_enabled);
        assert!(ecam.nvme_endpoint_state().command_bus_master_enabled);
        assert_eq!(
            ecam.mmio_target(machine::PCIE_MMIO_32.base),
            Some(PcieMmioTarget {
                bdf: NVME_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        assert_eq!(
            ecam.mmio_target(machine::PCIE_MMIO_32.base + 0x1234)
                .map(|t| t.offset),
            Some(0x1234)
        );
        assert_eq!(
            ecam.mmio_target(machine::PCIE_MMIO_32.base + u64::from(NVME_BAR0_SIZE)),
            None
        );
    }

    #[test]
    fn mmio_target_mru_hits_same_bar_and_invalidates_on_config_write() {
        let mut ecam = PcieEcam::new();
        let bar0 = ecam_offset(0, 1, 0, REG_BAR0);
        let bar1 = ecam_offset(0, 1, 0, REG_BAR0 + 4);
        let cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);
        let base = machine::PCIE_MMIO_32.base;

        ecam.cfg_write(bar0, 4, base);
        ecam.cfg_write(bar1, 4, 0);
        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
        assert_eq!(ecam.mmio_mru.get(), None);

        assert_eq!(
            ecam.mmio_target(base),
            Some(PcieMmioTarget {
                bdf: NVME_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        let cached = ecam.mmio_mru.get().expect("mmio target cache populated");
        assert_eq!(cached.base, base);
        assert_eq!(cached.end, base + u64::from(NVME_BAR0_SIZE));
        assert_eq!(ecam.mmio_target(base - 0x10), None);

        assert_eq!(
            ecam.mmio_target(base + 0x40),
            Some(PcieMmioTarget {
                bdf: NVME_BDF,
                bar_index: 0,
                offset: 0x40,
            })
        );
        assert_eq!(ecam.mmio_mru.get(), Some(cached));

        ecam.cfg_write(cmd, 2, 0);
        assert_eq!(ecam.mmio_mru.get(), None);
        assert_eq!(ecam.mmio_target(base), None);
    }

    #[test]
    fn xhci_command_enable_does_not_satisfy_nvme_command_or_decode() {
        let mut ecam = PcieEcam::new();
        let nvme_bar0 = ecam_offset(0, 1, 0, REG_BAR0);
        let nvme_cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);
        let xhci_bar0 = ecam_offset(0, 2, 0, REG_BAR0);
        let xhci_bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
        let xhci_cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
        let nvme_base = machine::PCIE_MMIO_32.base;
        let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

        // Given: NVMe has a BAR0 base, while only xHCI has command bits enabled.
        ecam.cfg_write(nvme_bar0, 4, nvme_base);
        ecam.cfg_write(xhci_bar0, 4, xhci_base);
        ecam.cfg_write(xhci_bar1, 4, 0);
        ecam.cfg_write(xhci_cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

        // Then: xHCI enablement remains separate from the NVMe endpoint.
        let nvme_state = ecam.nvme_endpoint_state();
        assert!(nvme_state.bar0_assigned);
        assert!(!nvme_state.command_memory_enabled);
        assert!(!nvme_state.command_bus_master_enabled);
        assert_eq!(ecam.mmio_target(nvme_base), None);
        assert_eq!(
            ecam.mmio_target(xhci_base),
            Some(PcieMmioTarget {
                bdf: XHCI_BDF,
                bar_index: 0,
                offset: 0,
            })
        );

        // When: NVMe command bits are written, its own BAR starts decoding.
        ecam.cfg_write(nvme_cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

        // Then: the NVMe target is enabled by NVMe's command register only.
        assert_eq!(
            ecam.mmio_target(nvme_base),
            Some(PcieMmioTarget {
                bdf: NVME_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
    }

    #[test]
    fn nvme_bar0_sizing_probe_does_not_decode_after_command_enable() {
        let mut ecam = PcieEcam::new();
        let bar0 = ecam_offset(0, 1, 0, REG_BAR0);
        let cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);

        // Given: firmware is probing BAR0 size, not assigning a real base.
        ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
        let sizing_readback = ecam.cfg_read(bar0, 4);
        let sizing_probe_base = sizing_readback & !0xF;
        assert!(!ecam.nvme_endpoint_state().bar0_assigned);

        // When: command memory/bus-master bits are enabled while the sizing
        // latch is still present.
        ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

        // Then: the sizing value is still not an assigned BAR and must not
        // decode as the NVMe MMIO aperture.
        assert!(!ecam.nvme_endpoint_state().bar0_assigned);
        assert_eq!(ecam.mmio_target(sizing_probe_base), None);
    }

    #[test]
    fn bar_sizing_returns_a_power_of_two_mask() {
        // Exercise the BAR sizing protocol directly: a 64 KiB 32-bit memory BAR.
        let mut bar = Bar::memory32(0x1_0000);
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

//! Split out of pcie.rs to keep files under 850 lines.

use super::*;

/// Bytes of config space per function (PCIe extended config space: 4 KiB).
pub const CFG_SPACE_SIZE: u64 = 0x1000;
/// Functions per device (3-bit function number).
pub const FUNCS_PER_DEVICE: u8 = 8;
/// Devices per bus (5-bit device number).
pub const DEVICES_PER_BUS: u8 = 32;

// ECAM address bit layout for `pci-host-ecam-generic`:
//   addr = base + (bus << 20 | dev << 15 | fn << 12 | reg)
// i.e. 8 bits bus, 5 bits device, 3 bits function, 12 bits register.
pub(crate) const SHIFT_BUS: u64 = 20;
pub(crate) const SHIFT_DEV: u64 = 15;
pub(crate) const SHIFT_FN: u64 = 12;
pub(crate) const MASK_REG: u64 = CFG_SPACE_SIZE - 1; // low 12 bits
pub(crate) const MASK_FN: u64 = 0x7; // 3 bits
pub(crate) const MASK_DEV: u64 = 0x1f; // 5 bits
pub(crate) const MASK_BUS: u64 = 0xff; // 8 bits

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
/// Interrupt Line (byte 0) and Interrupt Pin (byte 1).
pub const REG_INTERRUPT_LINE_PIN: u16 = 0x3c;

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

/// Guest-programmed standard MSI state for the Intel HDA endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HdaMsiConfig {
    pub enabled: bool,
    pub address: u64,
    pub data: u32,
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
pub(crate) const NVME_PM_CAP_BYTES: [u8; 8] = [0x01, 0x00, 0x03, 0x00, 0x08, 0x00, 0x00, 0x00];
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
pub(crate) const XHCI_PCIE_CAP_BYTES: [u8; 20] = [
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

/// `BRIDGEVM_TRACE_VENUS_START=1`: log ECAM config-space accesses to the
/// virtio-gpu function. The venus KMD crashes before its first virtio
/// common-config access, so the PCI-config layer is the only device surface
/// that can still witness its last action. First 256 accesses then sampled.
pub(crate) fn venus_start_trace_cfg(what: &str, reg: u16, size: u8, value: u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    if !crate::virtio_gpu_trace::venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    // Unsampled: config traffic is a few hundred accesses per boot and the
    // KMD's very last pre-crash access must not be sampled away.
    println!("venus-start: gpu {what} reg={reg:#x} size={size} value={value:#x} n={n}");
}
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
/// One configuration vector plus one vector per virtio-gpu queue.
///
/// This matches QEMU's virtio-gpu PCI transport and the viogpu3d KMD's fixed
/// assignment: config=0, controlq=1, cursorq=2.
pub const VIRTIO_GPU_MSIX_VECTOR_COUNT: u16 = 3;
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

// ---- Intel ICH6 High Definition Audio endpoint (00:07.0) -----------------

/// Free slot used for the opt-in Intel HDA controller.
pub const HDA_BDF: (u8, u8, u8) = (0, 7, 0);
pub const HDA_VENDOR_ID: u16 = 0x8086;
pub const HDA_DEVICE_ID: u16 = 0x2668;
/// Multimedia / High Definition Audio / programming interface 0.
pub const HDA_CLASS_CODE: u32 = 0x0004_0300;
pub const HDA_REVISION: u8 = 0x01;
pub const HDA_BAR0_SIZE: u32 = crate::hda::BAR_SIZE;
/// BridgeVM/QEMU-compatible subsystem identity, matching the other emulated
/// PCI endpoints rather than claiming a physical Intel board subsystem.
pub const HDA_SUBSYSTEM_VENDOR_ID: u16 = 0x1af4;
pub const HDA_SUBSYSTEM_ID: u16 = 0x1100;
/// QEMU's intel-hda model and Windows hdaudbus.sys expect the standard MSI
/// capability at this fixed absolute config-space offset.
pub const HDA_MSI_CAP_OFFSET: u8 = 0x60;
/// 64-bit, single-vector standard MSI capability. Message Control starts at
/// 0x0080 (64-bit address capable, MSI disabled); address and data start zero.
pub const HDA_MSI_CAP_BYTES: [u8; 14] = [
    0x05, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The value an ECAM read returns when no device answers: all-ones. Firmware
/// treats a `0xFFFF_FFFF` vendor/device read as "slot empty".
pub const NO_DEVICE: u64 = 0xFFFF_FFFF;

/// A single modelled config-space function. Today the only one is the host
/// bridge; NVMe / virtio-pci endpoints add more.
#[derive(Debug, Clone)]
pub(crate) struct Function {
    pub(crate) bdf: (u8, u8, u8),
    pub(crate) vendor_device: u32,
    pub(crate) revision_class: u32,
    pub(crate) subsystem_ids: u32,
    /// The mutable command register (low 16 bits) — bit-masked on write.
    pub(crate) command: u16,
    /// BAR latch values. A `0` size mask means "this BAR is unimplemented", so
    /// it always reads back `0` and ignores the all-ones sizing probe.
    pub(crate) bars: [Bar; NUM_BARS],
    /// Offset of the first capability in config space, or `0` for none.
    pub(crate) cap_ptr: u8,
    /// PCI Interrupt Pin byte (0 = none, 1 = INTA, ...).
    pub(crate) interrupt_pin: u8,
    /// Raw capability bytes addressed by `cap_ptr` (sparse, by byte offset).
    pub(crate) cap_bytes: Vec<(u16, u8)>,
}

/// One Base Address Register and the size of the region it can decode.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Bar {
    /// Latched BAR value (low config/type bits OR'd with the programmed base).
    pub(crate) value: u32,
    /// Size mask: `!(size - 1)` for a power-of-two `size`, or `0` if the BAR is
    /// unimplemented. During the sizing probe the device returns this mask.
    pub(crate) size_mask: u32,
    /// Non-writable low type bits (memory/IO, 32/64-bit, prefetch) kept across
    /// a base re-program and the sizing probe.
    pub(crate) type_bits: u32,
    /// Whether the last write was the all-ones BAR sizing probe.  Inferring
    /// this from `value == size_mask | type_bits` is incorrect because a valid
    /// address at the top of an aperture can have exactly that bit pattern.
    pub(crate) sizing_probe: bool,
    pub(crate) kind: BarKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BarKind {
    #[default]
    Memory32,
    Memory64Low,
    Memory64High,
    Io,
}

impl Bar {
    /// Construct a 32-bit, non-prefetchable memory BAR with a power-of-two size.
    pub(crate) fn memory32(size: u32) -> Self {
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
            sizing_probe: false,
            kind: BarKind::Memory32,
        }
    }

    pub(crate) fn memory64(size: u32) -> (Self, Self) {
        Self::memory64_with_type_bits(size, 0x4)
    }

    pub(crate) fn memory64_prefetchable(size: u32) -> (Self, Self) {
        Self::memory64_with_type_bits(size, 0x0c)
    }

    pub(crate) fn memory64_with_type_bits(size: u32, low_type_bits: u32) -> (Self, Self) {
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
                sizing_probe: false,
                kind: BarKind::Memory64Low,
            },
            Self {
                value: 0,
                size_mask: 0xFFFF_FFFF,
                type_bits: 0,
                sizing_probe: false,
                kind: BarKind::Memory64High,
            },
        )
    }

    /// Construct an I/O BAR with a power-of-two size.
    pub(crate) fn io(size: u32) -> Self {
        assert!(size >= 0x4, "PCI I/O BAR size must be at least 4 bytes");
        assert!(
            size.is_power_of_two(),
            "PCI I/O BAR size must be a power of two"
        );
        Self {
            value: 0,
            size_mask: !(size - 1),
            type_bits: 0x1,
            sizing_probe: false,
            kind: BarKind::Io,
        }
    }

    /// Read back the BAR. After an all-ones sizing write the latched value is
    /// the size mask; otherwise it is the programmed base. Unimplemented BARs
    /// always read `0`.
    pub(crate) fn read(&self) -> u32 {
        if self.size_mask == 0 {
            0
        } else {
            self.value
        }
    }

    /// Apply a 32-bit BAR write. Writing all-ones latches the size mask (the
    /// sizing protocol); any other value latches the base with the type bits
    /// preserved.
    pub(crate) fn write(&mut self, value: u32) {
        if self.size_mask == 0 {
            return; // unimplemented: writes are dropped
        }
        if value == 0xFFFF_FFFF {
            // Sizing probe: report `size_mask | type_bits` on read-back.
            self.sizing_probe = true;
            self.value = self.size_mask | self.type_bits;
        } else {
            // Program a base: only the address bits above the size are kept.
            self.sizing_probe = false;
            self.value = (value & self.size_mask) | self.type_bits;
        }
    }

    /// Size of the decoded BAR region, or zero if unimplemented.
    pub(crate) fn size(&self) -> u64 {
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
    pub(crate) fn base(&self) -> Option<u64> {
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

    pub(crate) fn assigned_base(&self) -> Option<u64> {
        let base = self.base()?;
        match self.kind {
            BarKind::Memory32 | BarKind::Memory64Low => {
                (base != 0 && !self.sizing_probe).then_some(base)
            }
            BarKind::Io => (!self.sizing_probe).then_some(base),
            BarKind::Memory64High => None,
        }
    }

    /// Offset into this BAR for `addr`, if the BAR currently decodes it.
    pub(crate) fn offset_of(&self, addr: u64) -> Option<u64> {
        let base = self.assigned_base()?;
        let size = self.size();
        let offset = addr.checked_sub(base)?;
        (offset < size).then_some(offset)
    }

    pub(crate) fn pio_offset_of(&self, port: u64) -> Option<u64> {
        (self.kind == BarKind::Io)
            .then(|| self.offset_of(port))
            .flatten()
    }
}

impl Function {
    pub(crate) fn memory64_assigned_base(&self, idx: usize) -> Option<u64> {
        let low = self.bars.get(idx)?;
        let high = self.bars.get(idx + 1)?;
        if low.kind != BarKind::Memory64Low || high.kind != BarKind::Memory64High {
            return None;
        }
        if low.sizing_probe || high.sizing_probe {
            return None;
        }
        let base = (u64::from(high.value) << 32) | low.base()?;
        (base != 0).then_some(base)
    }

    /// The QEMU PCIe host bridge at `00:00.0`: type-0 header, no BARs, no
    /// capabilities. A clean, enumerable root complex.
    pub(crate) fn host_bridge() -> Self {
        Self {
            bdf: (0, 0, 0),
            vendor_device: (u32::from(HOST_BRIDGE_DEVICE_ID) << 16)
                | u32::from(HOST_BRIDGE_VENDOR_ID),
            revision_class: (HOST_BRIDGE_CLASS_CODE << 8) | u32::from(HOST_BRIDGE_REVISION),
            subsystem_ids: 0,
            command: 0,
            bars: [Bar::default(); NUM_BARS],
            cap_ptr: 0,
            interrupt_pin: 0,
            cap_bytes: Vec::new(),
        }
    }

    /// The first NVMe storage endpoint at `00:01.0`.
    pub(crate) fn nvme() -> Self {
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
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    /// QEMU-oracle virtio block installer media endpoint at `00:03.0`.
    pub(crate) fn virtio_blk() -> Self {
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
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    /// Modern-only virtio network endpoint at `00:04.0`.
    pub(crate) fn virtio_net() -> Self {
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
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    /// Modern-only virtio GPU endpoint at `00:05.0`.
    pub(crate) fn virtio_gpu(host_visible_bar_size: Option<u64>, pci_device_id: u16) -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        bars[1] = Bar::memory32(VIRTIO_GPU_BAR1_SIZE);
        if let Some(size) = host_visible_bar_size {
            let size32 = u32::try_from(size)
                .expect("virtio-gpu host-visible BAR size must currently fit in 32 bits");
            let (mut bar2, bar3) = Bar::memory64_prefetchable(size32);
            // PCI BAR type bits are read-only and visible even while the base
            // address is zero.  Leaving BAR2 at an all-zero power-on value
            // makes firmware treat it as a 32-bit/non-prefetchable slot (or
            // skip it entirely) before the sizing probe, so the 64-bit BAR pair
            // never receives an address.
            bar2.value = bar2.type_bits;
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
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    /// Modern-only virtio console endpoint at `00:06.0`.
    pub(crate) fn virtio_console() -> Self {
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
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    pub(crate) fn xhci() -> Self {
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
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    pub(crate) fn hda() -> Self {
        let mut bars = [Bar::default(); NUM_BARS];
        bars[0] = Bar::memory32(HDA_BAR0_SIZE);
        let cap_bytes = HDA_MSI_CAP_BYTES
            .into_iter()
            .enumerate()
            .map(|(i, byte)| (u16::from(HDA_MSI_CAP_OFFSET) + i as u16, byte))
            .collect();
        Self {
            bdf: HDA_BDF,
            vendor_device: (u32::from(HDA_DEVICE_ID) << 16) | u32::from(HDA_VENDOR_ID),
            revision_class: (HDA_CLASS_CODE << 8) | u32::from(HDA_REVISION),
            subsystem_ids: (u32::from(HDA_SUBSYSTEM_ID) << 16) | u32::from(HDA_SUBSYSTEM_VENDOR_ID),
            command: 0,
            bars,
            cap_ptr: HDA_MSI_CAP_OFFSET,
            // MSI-only: our platform describes no legacy INTx GSI routing for
            // PCI slots (all other functions are pin 0), so advertising INTA —
            // as QEMU can, because it ships an ACPI _PRT — makes Windows try to
            // reserve an unroutable IRQ line and fail with a resource conflict.
            interrupt_pin: 0,
            cap_bytes,
        }
    }

    pub(crate) fn mmio_target_of_bar(&self, idx: usize, gpa: u64) -> Option<PcieMmioTargetMru> {
        let bar = self.bars.get(idx)?;
        let (base, size) = match bar.kind {
            BarKind::Memory32 => (bar.assigned_base()?, bar.size()),
            BarKind::Memory64Low => (self.memory64_assigned_base(idx)?, bar.size()),
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
    pub(crate) fn read_dword(&self, reg: u16) -> u32 {
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
            REG_INTERRUPT_LINE_PIN => u32::from(self.interrupt_pin) << 8,
            _ if (REG_BAR0..REG_BAR0 + (NUM_BARS as u16) * 4).contains(&reg) => {
                let idx = ((reg - REG_BAR0) / 4) as usize;
                self.bars[idx].read()
            }
            _ => self.read_capability_dword(reg),
        }
    }
}

// ---- The ECAM device --------------------------------------------------------

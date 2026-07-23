//! Per-endpoint identity: BDF, vendor/device/class, BAR sizes, capability and MSI-X layout.

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

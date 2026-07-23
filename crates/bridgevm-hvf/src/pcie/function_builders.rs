//! Constructors that assemble each modelled endpoint config space.

use super::*;

impl Function {
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
}

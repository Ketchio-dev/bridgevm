use bridgevm_hvf::{machine, pcie, virtio_blk::INSTALLER_ISO_SLOT};

pub(super) fn print_device_shape(
    pci_installer_iso_attached: bool,
    legacy_mmio_installer_iso_attached: bool,
    nvme_namespace_bytes: u64,
) {
    println!("BridgeVM device shape:");
    for line in device_shape_lines(
        pci_installer_iso_attached,
        legacy_mmio_installer_iso_attached,
        nvme_namespace_bytes,
    ) {
        println!("  {line}");
    }
}

fn device_shape_lines(
    pci_installer_iso_attached: bool,
    legacy_mmio_installer_iso_attached: bool,
    nvme_namespace_bytes: u64,
) -> Vec<String> {
    let (nvme_bus, nvme_dev, nvme_func) = pcie::NVME_BDF;
    let iso_base = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
    let iso_slot = match u32::try_from(INSTALLER_ISO_SLOT) {
        Ok(slot) => slot,
        Err(_) => panic!("installer ISO slot does not fit a virtio-mmio SPI index"),
    };
    let iso_spi = machine::virtio_mmio_spi(iso_slot);
    let iso_intid = machine::spi_to_intid(iso_spi);

    vec![
        format!(
            "00:00.0 pci-host-ecam-generic vendor={:#06x} device={:#06x} class={:#08x}",
            pcie::HOST_BRIDGE_VENDOR_ID,
            pcie::HOST_BRIDGE_DEVICE_ID,
            pcie::HOST_BRIDGE_CLASS_CODE
        ),
        format!(
            "{nvme_bus:02x}:{nvme_dev:02x}.{nvme_func} nvme vendor={:#06x} device={:#06x} class={:#08x}",
            pcie::NVME_VENDOR_ID,
            pcie::NVME_DEVICE_ID,
            pcie::NVME_CLASS_CODE
        ),
        format!("boot-media nvme namespace bytes={nvme_namespace_bytes}"),
        format!(
            "boot-media installer ISO fallback: virtio-mmio slot {INSTALLER_ISO_SLOT} base={iso_base:#x} spi={iso_spi} intid={iso_intid} attached={legacy_mmio_installer_iso_attached}"
        ),
        format!(
            "boot-media installer ISO: {bus:02x}:{dev:02x}.{func} virtio-blk-pci vendor={vendor:#06x} device={device:#06x} class={class:#08x} bar0_io_size={bar0:#x} bar1_msix_size={bar1:#x} bar4_modern_mmio_size={bar4:#x} attached={pci_installer_iso_attached}",
            bus = pcie::VIRTIO_BLK_BDF.0,
            dev = pcie::VIRTIO_BLK_BDF.1,
            func = pcie::VIRTIO_BLK_BDF.2,
            vendor = pcie::VIRTIO_BLK_VENDOR_ID,
            device = pcie::VIRTIO_BLK_DEVICE_ID,
            class = pcie::VIRTIO_BLK_CLASS_CODE,
            bar0 = pcie::VIRTIO_BLK_BAR0_SIZE,
            bar1 = pcie::VIRTIO_BLK_BAR1_SIZE,
            bar4 = pcie::VIRTIO_BLK_BAR4_SIZE,
        ),
        "qemu oracle parity: virtio-net-pci 00:01.0 absent (BridgeVM uses NVMe at 00:01.0)"
            .to_string(),
        "qemu oracle parity: qemu-xhci 00:02.0 absent".to_string(),
        "qemu oracle parity: legacy virtio-mmio slot 31 kept as installer ISO fallback"
            .to_string(),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn summary_names_pci_boot_media_and_legacy_mmio_fallback() {
        let lines = super::device_shape_lines(true, false, 16 * 1024 * 1024);

        assert!(lines
            .iter()
            .any(|line| line.contains("00:00.0 pci-host-ecam-generic")));
        assert!(lines.iter().any(|line| line.contains("00:01.0 nvme")));
        assert!(lines
            .iter()
            .any(|line| line.contains("virtio-mmio slot 31")));
        assert!(lines
            .iter()
            .any(|line| line.contains("virtio-mmio slot 31") && line.contains("attached=false")));
        assert!(lines.iter().any(|line| line.contains("legacy virtio-mmio")));
        assert!(lines.iter().any(|line| line
            .contains("00:03.0 virtio-blk-pci vendor=0x1af4 device=0x1001 class=0x010000")
            && line.contains("bar0_io_size=0x80")
            && line.contains("bar4_modern_mmio_size=0x4000")
            && line.contains("attached=true")));
        assert!(lines
            .iter()
            .any(|line| line.contains("qemu-xhci 00:02.0 absent")));

        let fallback_lines = super::device_shape_lines(false, true, 16 * 1024 * 1024);
        assert!(fallback_lines
            .iter()
            .any(|line| line.contains("virtio-mmio slot 31") && line.contains("attached=true")));
        assert!(
            fallback_lines
                .iter()
                .any(|line| line.contains("00:03.0 virtio-blk-pci")
                    && line.contains("attached=false"))
        );
    }
}

use bridgevm_hvf::{machine, pcie, virtio_blk::INSTALLER_ISO_SLOT};

pub(super) fn print_device_shape(installer_iso_attached: bool, nvme_namespace_bytes: u64) {
    println!("BridgeVM device shape:");
    for line in device_shape_lines(installer_iso_attached, nvme_namespace_bytes) {
        println!("  {line}");
    }
}

fn device_shape_lines(installer_iso_attached: bool, nvme_namespace_bytes: u64) -> Vec<String> {
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
            "boot-media installer ISO: virtio-mmio slot {INSTALLER_ISO_SLOT} base={iso_base:#x} spi={iso_spi} intid={iso_intid} attached={installer_iso_attached}"
        ),
        "qemu oracle parity: virtio-net-pci 00:01.0 absent (BridgeVM uses NVMe at 00:01.0)"
            .to_string(),
        "qemu oracle parity: qemu-xhci 00:02.0 absent".to_string(),
        "qemu oracle parity: virtio-blk-pci 00:03.0 absent (BridgeVM installer ISO uses legacy virtio-mmio)"
            .to_string(),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn summary_names_current_bridgevm_and_qemu_boot_media_shapes() {
        let lines = super::device_shape_lines(true, 16 * 1024 * 1024);

        assert!(lines
            .iter()
            .any(|line| line.contains("00:00.0 pci-host-ecam-generic")));
        assert!(lines.iter().any(|line| line.contains("00:01.0 nvme")));
        assert!(lines
            .iter()
            .any(|line| line.contains("virtio-mmio slot 31")));
        assert!(lines
            .iter()
            .any(|line| line.contains("qemu-xhci 00:02.0 absent")));
        assert!(lines
            .iter()
            .any(|line| line.contains("virtio-blk-pci 00:03.0 absent")));
    }
}

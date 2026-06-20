use bridgevm_hvf::nvme;
use bridgevm_hvf::pcie::{self, PcieMmioTarget, PciePioTarget};

pub(super) fn pcie_mmio_register_name(
    target: Option<PcieMmioTarget>,
    aperture_offset: u64,
) -> String {
    let Some(target) = target else {
        return format!("pcie-mmio-32+{aperture_offset:#x}");
    };
    pcie_bar_register_label(target)
}

pub(super) fn pcie_pio_register_name(target: Option<PciePioTarget>, port: u64) -> String {
    let Some(target) = target else {
        return format!("pcie-pio+{port:#x}");
    };
    pcie_pio_bar_register_label(target)
}

fn pcie_bar_prefix(bdf: (u8, u8, u8), bar_index: usize, offset: u64) -> String {
    let (bus, device, function) = bdf;
    format!("{bus:02x}:{device:02x}.{function} BAR{bar_index}+{offset:#x}")
}

fn pcie_bar_register_label(target: PcieMmioTarget) -> String {
    let prefix = pcie_bar_prefix(target.bdf, target.bar_index, target.offset);
    match pcie_bar_register_name(target) {
        Some(name) => format!("{prefix} {name}"),
        None => prefix,
    }
}

fn pcie_pio_bar_register_label(target: PciePioTarget) -> String {
    let prefix = pcie_bar_prefix(target.bdf, target.bar_index, target.offset);
    match pcie_pio_bar_register_name(target) {
        Some(name) => format!("{prefix} {name}"),
        None => prefix,
    }
}

fn pcie_pio_bar_register_name(target: PciePioTarget) -> Option<String> {
    match (target.bdf, target.bar_index) {
        (pcie::VIRTIO_BLK_BDF, 0) => Some(virtio_pci_bar0_io_register_name(target.offset)),
        _ => None,
    }
}

fn virtio_pci_bar0_io_register_name(offset: u64) -> String {
    match offset {
        0x00 => "virtio.legacy.device_features".to_string(),
        0x04 => "virtio.legacy.driver_features".to_string(),
        0x08 => "virtio.legacy.queue_pfn".to_string(),
        0x0c => "virtio.legacy.queue_size".to_string(),
        0x0e => "virtio.legacy.queue_select".to_string(),
        0x10 => "virtio.legacy.queue_notify".to_string(),
        0x12 => "virtio.legacy.device_status".to_string(),
        0x13 => "virtio.legacy.isr".to_string(),
        o if o >= 0x14 => format!("virtio.legacy.device_config+{:#x}", o - 0x14),
        _ => format!("virtio.legacy+{offset:#x}"),
    }
}

fn pcie_bar_register_name(target: PcieMmioTarget) -> Option<String> {
    match (target.bdf, target.bar_index) {
        (pcie::NVME_BDF, 0) => Some(nvme_bar0_register_name(target.offset)),
        (pcie::VIRTIO_BLK_BDF, 4) => Some(virtio_pci_bar4_register_name(target.offset)),
        _ => None,
    }
}

fn nvme_bar0_register_name(offset: u64) -> String {
    match offset {
        nvme::REG_CAP => "nvme.CAP".to_string(),
        o if o == nvme::REG_CAP + 4 => "nvme.CAP+4".to_string(),
        nvme::REG_VS => "nvme.VS".to_string(),
        nvme::REG_INTMS => "nvme.INTMS".to_string(),
        nvme::REG_INTMC => "nvme.INTMC".to_string(),
        nvme::REG_CC => "nvme.CC".to_string(),
        nvme::REG_CSTS => "nvme.CSTS".to_string(),
        nvme::REG_AQA => "nvme.AQA".to_string(),
        nvme::REG_ASQ => "nvme.ASQ".to_string(),
        o if o == nvme::REG_ASQ + 4 => "nvme.ASQ+4".to_string(),
        nvme::REG_ACQ => "nvme.ACQ".to_string(),
        o if o == nvme::REG_ACQ + 4 => "nvme.ACQ+4".to_string(),
        nvme::REG_CMBLOC => "nvme.CMBLOC".to_string(),
        nvme::REG_CMBSZ => "nvme.CMBSZ".to_string(),
        o if (nvme::REG_DOORBELL_BASE..nvme::REG_DOORBELL_END).contains(&o) && o % 4 == 0 => {
            let index = (o - nvme::REG_DOORBELL_BASE) / 4;
            let qid = index / 2;
            if index % 2 == 0 {
                format!("nvme.SQ{qid}TDBL")
            } else {
                format!("nvme.CQ{qid}HDBL")
            }
        }
        o if (u64::from(pcie::NVME_MSIX_TABLE_OFFSET)
            ..u64::from(pcie::NVME_MSIX_TABLE_OFFSET)
                + u64::from(pcie::NVME_MSIX_VECTOR_COUNT) * 16)
            .contains(&o) =>
        {
            let table_off = o - u64::from(pcie::NVME_MSIX_TABLE_OFFSET);
            let vector = table_off / 16;
            let field = match table_off % 16 {
                0..=3 => "addr_lo",
                4..=7 => "addr_hi",
                8..=11 => "data",
                _ => "vector_ctl",
            };
            format!("nvme.msix.table[{vector}].{field}")
        }
        o if (u64::from(pcie::NVME_MSIX_PBA_OFFSET)..u64::from(pcie::NVME_MSIX_PBA_OFFSET) + 8)
            .contains(&o) =>
        {
            "nvme.msix.pba".to_string()
        }
        _ => format!("nvme.bar0+{offset:#x}"),
    }
}

fn virtio_pci_bar4_register_name(offset: u64) -> String {
    match offset {
        0x0000 => "virtio.common.device_feature_select".to_string(),
        0x0004 => "virtio.common.device_feature".to_string(),
        0x0008 => "virtio.common.driver_feature_select".to_string(),
        0x000c => "virtio.common.driver_feature".to_string(),
        0x0010 => "virtio.common.msix_config".to_string(),
        0x0012 => "virtio.common.num_queues".to_string(),
        0x0014 => "virtio.common.device_status".to_string(),
        0x0015 => "virtio.common.config_generation".to_string(),
        0x0016 => "virtio.common.queue_select".to_string(),
        0x0018 => "virtio.common.queue_size".to_string(),
        0x001a => "virtio.common.queue_msix_vector".to_string(),
        0x001c => "virtio.common.queue_enable".to_string(),
        0x001e => "virtio.common.queue_notify_off".to_string(),
        0x0020 => "virtio.common.queue_desc_low".to_string(),
        0x0024 => "virtio.common.queue_desc_high".to_string(),
        0x0028 => "virtio.common.queue_driver_low".to_string(),
        0x002c => "virtio.common.queue_driver_high".to_string(),
        0x0030 => "virtio.common.queue_device_low".to_string(),
        0x0034 => "virtio.common.queue_device_high".to_string(),
        0x1000 => "virtio.isr".to_string(),
        o if (0x2000..0x3000).contains(&o) => format!("virtio.device+{:#x}", o - 0x2000),
        o if (0x3000..0x4000).contains(&o) => format!("virtio.notify+{:#x}", o - 0x3000),
        _ => format!("virtio.bar4+{offset:#x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_nvme_doorbell_registers() {
        assert_eq!(nvme_bar0_register_name(0x1000), "nvme.SQ0TDBL");
        assert_eq!(nvme_bar0_register_name(0x1004), "nvme.CQ0HDBL");
        assert_eq!(nvme_bar0_register_name(0x1008), "nvme.SQ1TDBL");
        assert_eq!(nvme_bar0_register_name(0x100c), "nvme.CQ1HDBL");
    }

    #[test]
    fn decodes_nvme_registers_and_msix_table() {
        assert_eq!(nvme_bar0_register_name(nvme::REG_CSTS), "nvme.CSTS");
        assert_eq!(nvme_bar0_register_name(nvme::REG_CMBLOC), "nvme.CMBLOC");
        assert_eq!(nvme_bar0_register_name(nvme::REG_CMBSZ), "nvme.CMBSZ");
        assert_eq!(
            nvme_bar0_register_name(u64::from(pcie::NVME_MSIX_TABLE_OFFSET) + 24),
            "nvme.msix.table[1].data"
        );
        assert_eq!(
            nvme_bar0_register_name(u64::from(pcie::NVME_MSIX_PBA_OFFSET)),
            "nvme.msix.pba"
        );
    }

    #[test]
    fn labels_programmed_bars_by_bdf_and_bar_offset() {
        for (target, aperture_offset, expected) in [
            (
                PcieMmioTarget {
                    bdf: pcie::NVME_BDF,
                    bar_index: 0,
                    offset: 0,
                },
                0x4000,
                "00:01.0 BAR0+0x0 nvme.CAP",
            ),
            (
                PcieMmioTarget {
                    bdf: pcie::VIRTIO_BLK_BDF,
                    bar_index: 4,
                    offset: 0,
                },
                0x8000,
                "00:03.0 BAR4+0x0 virtio.common.device_feature_select",
            ),
        ] {
            assert_eq!(
                pcie_mmio_register_name(Some(target), aperture_offset),
                expected
            );
        }
    }

    #[test]
    fn labels_programmed_pio_bars_by_bdf_and_bar_offset() {
        assert_eq!(
            pcie_pio_register_name(
                Some(PciePioTarget {
                    bdf: pcie::VIRTIO_BLK_BDF,
                    bar_index: 0,
                    offset: 0x10,
                }),
                0x10,
            ),
            "00:03.0 BAR0+0x10 virtio.legacy.queue_notify"
        );
    }

    #[test]
    fn falls_back_to_aperture_offset_without_a_resolved_target() {
        assert_eq!(pcie_mmio_register_name(None, 0x4000), "pcie-mmio-32+0x4000");
        assert_eq!(pcie_pio_register_name(None, 0x40), "pcie-pio+0x40");
    }
}

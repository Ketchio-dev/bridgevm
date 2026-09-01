use super::*;
use crate::pcie::{
    HDA_BDF, NVME_BDF, VIRTIO_BLK_BDF, VIRTIO_CONSOLE_BDF, VIRTIO_GPU_BDF, VIRTIO_NET_BDF, XHCI_BDF,
};

fn ecam_offset(bdf: (u8, u8, u8)) -> u64 {
    (u64::from(bdf.0) << 20) | (u64::from(bdf.1) << 15) | (u64::from(bdf.2) << 12)
}

fn vendor_id(platform: &mut BridgeVmPcPlatform, bdf: (u8, u8, u8)) -> MmioOutcome {
    platform.on_mmio_without_dma(
        board::PCIE_ECAM.base + ecam_offset(bdf),
        MmioOp::Read { size: 2 },
    )
}

#[test]
fn ecam_exposes_only_endpoints_with_runtime_models() {
    let mut platform = BridgeVmPcPlatform::new();
    for bdf in [NVME_BDF, XHCI_BDF] {
        assert_ne!(
            vendor_id(&mut platform, bdf),
            MmioOutcome::ReadValue(0xffff),
            "missing {bdf:?}"
        );
    }
    for bdf in [
        VIRTIO_BLK_BDF,
        VIRTIO_NET_BDF,
        VIRTIO_GPU_BDF,
        VIRTIO_CONSOLE_BDF,
        HDA_BDF,
    ] {
        assert_eq!(
            vendor_id(&mut platform, bdf),
            MmioOutcome::ReadValue(0xffff),
            "unexpected {bdf:?}"
        );
    }
}

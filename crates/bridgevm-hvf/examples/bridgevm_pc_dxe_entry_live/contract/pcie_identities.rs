use super::FUNCTION_COUNT;
use bridgevm_hvf::pcie;

pub(super) fn expected() -> [u32; FUNCTION_COUNT] {
    [
        (u32::from(pcie::HOST_BRIDGE_DEVICE_ID) << 16) | u32::from(pcie::HOST_BRIDGE_VENDOR_ID),
        (u32::from(pcie::NVME_DEVICE_ID) << 16) | u32::from(pcie::NVME_VENDOR_ID),
        (u32::from(pcie::XHCI_DEVICE_ID) << 16) | u32::from(pcie::XHCI_VENDOR_ID),
        (u32::from(pcie::VIRTIO_BLK_DEVICE_ID) << 16) | u32::from(pcie::VIRTIO_BLK_VENDOR_ID),
        (u32::from(pcie::VIRTIO_NET_DEVICE_ID) << 16) | u32::from(pcie::VIRTIO_NET_VENDOR_ID),
        (u32::from(pcie::VIRTIO_GPU_DEVICE_ID) << 16) | u32::from(pcie::VIRTIO_GPU_VENDOR_ID),
        (u32::from(pcie::VIRTIO_CONSOLE_DEVICE_ID) << 16)
            | u32::from(pcie::VIRTIO_CONSOLE_VENDOR_ID),
        (u32::from(pcie::HDA_DEVICE_ID) << 16) | u32::from(pcie::HDA_VENDOR_ID),
    ]
}

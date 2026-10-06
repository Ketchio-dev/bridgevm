use super::FUNCTION_COUNT;
use bridgevm_hvf::pcie;

pub(super) fn expected() -> [u32; FUNCTION_COUNT] {
    [
        (u32::from(pcie::HOST_BRIDGE_DEVICE_ID) << 16) | u32::from(pcie::HOST_BRIDGE_VENDOR_ID),
        (u32::from(pcie::NVME_DEVICE_ID) << 16) | u32::from(pcie::NVME_VENDOR_ID),
        (u32::from(pcie::XHCI_DEVICE_ID) << 16) | u32::from(pcie::XHCI_VENDOR_ID),
    ]
}

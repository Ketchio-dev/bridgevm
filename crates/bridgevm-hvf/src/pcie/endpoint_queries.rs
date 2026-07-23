//! Host-side queries about endpoint programming state.

use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PcieNvmeEndpointState {
    pub advertised: bool,
    pub command_memory_enabled: bool,
    pub command_bus_master_enabled: bool,
    pub bar0_assigned: bool,
}

impl PcieEcam {
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

    /// Standard MSI programming for the opt-in HDA endpoint.
    pub fn hda_msi_config(&self) -> HdaMsiConfig {
        self.function_at(HDA_BDF)
            .and_then(Function::msi_config)
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
        func.memory64_assigned_base(2)
    }

    pub fn virtio_gpu_host_visible_bar_size(&self) -> Option<u64> {
        self.function_at(VIRTIO_GPU_BDF)
            .map(|func| func.bars[2].size())
            .filter(|size| *size != 0)
    }
}

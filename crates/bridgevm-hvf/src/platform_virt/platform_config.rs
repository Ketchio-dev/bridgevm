//! Caller-facing machine shape: which devices are present and which net backend to use.

use crate::dtb::VirtFdtConfig;
use crate::pcie::VIRTIO_GPU_DEVICE_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtPlatformDeviceConfig {
    pub xhci_present: bool,
    pub hda_present: bool,
    pub virtio_boot_media_present: bool,
    pub virtio_net_present: bool,
    pub virtio_gpu_present: bool,
    pub virtio_console_present: bool,
    pub virtio_gpu_pci_device_id: u16,
    pub virtio_net_backend: VirtioNetBackendKind,
    pub legacy_virtio_mmio_present: bool,
    pub ramfb_present: bool,
    pub tpm_tis_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioNetBackendKind {
    Nat,
    Loopback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtPlatformConfig {
    pub fdt: VirtFdtConfig,
    pub devices: VirtPlatformDeviceConfig,
}

impl Default for VirtPlatformDeviceConfig {
    fn default() -> Self {
        Self {
            xhci_present: true,
            hda_present: false,
            virtio_boot_media_present: true,
            virtio_net_present: false,
            virtio_gpu_present: false,
            virtio_console_present: false,
            virtio_gpu_pci_device_id: VIRTIO_GPU_DEVICE_ID,
            virtio_net_backend: VirtioNetBackendKind::Nat,
            legacy_virtio_mmio_present: true,
            ramfb_present: false,
            tpm_tis_present: false,
        }
    }
}

impl VirtioNetBackendKind {
    pub fn from_env_value(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Nat;
        };
        let value = value.trim();
        if value.eq_ignore_ascii_case("nat") {
            Self::Nat
        } else if value.eq_ignore_ascii_case("loopback") {
            Self::Loopback
        } else {
            panic!("BRIDGEVM_VIRTIO_NET_BACKEND must be 'nat' or 'loopback'");
        }
    }
}

impl VirtPlatformConfig {
    pub fn new(fdt: VirtFdtConfig) -> Self {
        Self {
            fdt,
            devices: VirtPlatformDeviceConfig::default(),
        }
    }

    pub fn with_ramfb(fdt: VirtFdtConfig) -> Self {
        let mut config = Self::new(fdt);
        config.devices.ramfb_present = true;
        config
    }
}

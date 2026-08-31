use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::pcie;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub role: &'static str,
    pub bdf: (u8, u8, u8),
    pub identity: u32,
}

const fn identity(vendor: u16, device: u16) -> u32 {
    ((device as u32) << 16) | vendor as u32
}

pub const ENDPOINTS: [Endpoint; 8] = [
    Endpoint {
        role: "host-bridge",
        bdf: (0, 0, 0),
        identity: identity(pcie::HOST_BRIDGE_VENDOR_ID, pcie::HOST_BRIDGE_DEVICE_ID),
    },
    Endpoint {
        role: "system-storage",
        bdf: pcie::NVME_BDF,
        identity: identity(pcie::NVME_VENDOR_ID, pcie::NVME_DEVICE_ID),
    },
    Endpoint {
        role: "usb-input",
        bdf: pcie::XHCI_BDF,
        identity: identity(pcie::XHCI_VENDOR_ID, pcie::XHCI_DEVICE_ID),
    },
    Endpoint {
        role: "installer-media",
        bdf: pcie::VIRTIO_BLK_BDF,
        identity: identity(pcie::VIRTIO_BLK_VENDOR_ID, pcie::VIRTIO_BLK_DEVICE_ID),
    },
    Endpoint {
        role: "network",
        bdf: pcie::VIRTIO_NET_BDF,
        identity: identity(pcie::VIRTIO_NET_VENDOR_ID, pcie::VIRTIO_NET_DEVICE_ID),
    },
    Endpoint {
        role: "display",
        bdf: pcie::VIRTIO_GPU_BDF,
        identity: identity(pcie::VIRTIO_GPU_VENDOR_ID, pcie::VIRTIO_GPU_DEVICE_ID),
    },
    Endpoint {
        role: "guest-agent",
        bdf: pcie::VIRTIO_CONSOLE_BDF,
        identity: identity(
            pcie::VIRTIO_CONSOLE_VENDOR_ID,
            pcie::VIRTIO_CONSOLE_DEVICE_ID,
        ),
    },
    Endpoint {
        role: "audio",
        bdf: pcie::HDA_BDF,
        identity: identity(pcie::HDA_VENDOR_ID, pcie::HDA_DEVICE_ID),
    },
];

pub const fn ecam_gpa(bdf: (u8, u8, u8)) -> u64 {
    board::PCIE_ECAM.base + ((bdf.0 as u64) << 20) + ((bdf.1 as u64) << 15) + ((bdf.2 as u64) << 12)
}

pub fn validate(observed: &[u64; ENDPOINTS.len()]) -> Result<(), String> {
    for (endpoint, value) in ENDPOINTS.iter().zip(observed) {
        if *value != u64::from(endpoint.identity) {
            return Err(format!(
                "{} at {:02x}:{:02x}.{} returned {value:#010x}; expected {:#010x}",
                endpoint.role, endpoint.bdf.0, endpoint.bdf.1, endpoint.bdf.2, endpoint.identity
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_order_matches_the_versioned_board_contract() {
        for (expected, declared) in ENDPOINTS.iter().skip(1).zip(board::PCI_DEVICES) {
            assert_eq!(expected.role, declared.role);
            assert_eq!(
                format!(
                    "{:02x}:{:02x}.{}",
                    expected.bdf.0, expected.bdf.1, expected.bdf.2
                ),
                declared.bdf
            );
        }
    }

    #[test]
    fn every_identity_read_is_inside_the_independent_ecam() {
        let mut gpas = ENDPOINTS.map(|endpoint| ecam_gpa(endpoint.bdf));
        assert!(gpas.iter().all(|gpa| board::PCIE_ECAM.contains(*gpa)));
        gpas.sort_unstable();
        assert!(gpas.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn mismatched_guest_identity_fails_closed() {
        let mut observed = ENDPOINTS.map(|endpoint| u64::from(endpoint.identity));
        observed[5] ^= 1;
        assert!(validate(&observed).is_err());
    }
}

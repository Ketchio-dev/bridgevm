use super::VIRTIO_BLK_MSIX_CAP_OFFSET;

const VIRTIO_PCI_CAP_VENDOR: u8 = 0x09;
const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

const VIRTIO_PCI_BAR4: u8 = 4;
const VIRTIO_PCI_CAP_COMMON_OFFSET: u8 = 0x40;
const VIRTIO_PCI_CAP_NOTIFY_OFFSET: u8 = 0x50;
const VIRTIO_PCI_CAP_ISR_OFFSET: u8 = 0x64;
const VIRTIO_PCI_CAP_DEVICE_OFFSET: u8 = 0x74;

const VIRTIO_PCI_COMMON_CFG_OFFSET: u32 = 0x0000;
const VIRTIO_PCI_ISR_CFG_OFFSET: u32 = 0x1000;
const VIRTIO_PCI_DEVICE_CFG_OFFSET: u32 = 0x2000;
const VIRTIO_PCI_NOTIFY_CFG_OFFSET: u32 = 0x3000;
const VIRTIO_PCI_CFG_REGION_SIZE: u32 = 0x1000;
const VIRTIO_PCI_NOTIFY_OFF_MULTIPLIER: u32 = 4;

pub(super) struct VirtioCapabilityList {
    pub cap_ptr: u8,
    pub cap_bytes: Vec<(u16, u8)>,
}

#[derive(Clone, Copy)]
struct VirtioPciCap {
    offset: u8,
    next: u8,
    cfg_type: u8,
    bar: u8,
    region_offset: u32,
    region_length: u32,
}

impl VirtioPciCap {
    const fn bytes(self) -> [u8; 16] {
        let offset = self.region_offset.to_le_bytes();
        let length = self.region_length.to_le_bytes();
        [
            VIRTIO_PCI_CAP_VENDOR,
            self.next,
            16,
            self.cfg_type,
            self.bar,
            0,
            0,
            0,
            offset[0],
            offset[1],
            offset[2],
            offset[3],
            length[0],
            length[1],
            length[2],
            length[3],
        ]
    }
}

pub(super) fn boot_media_capability_list() -> VirtioCapabilityList {
    capability_list(VIRTIO_BLK_MSIX_CAP_OFFSET)
}

pub(super) fn capability_list(msix_cap_offset: u8) -> VirtioCapabilityList {
    let common = VirtioPciCap {
        offset: VIRTIO_PCI_CAP_COMMON_OFFSET,
        next: VIRTIO_PCI_CAP_NOTIFY_OFFSET,
        cfg_type: VIRTIO_PCI_CAP_COMMON_CFG,
        bar: VIRTIO_PCI_BAR4,
        region_offset: VIRTIO_PCI_COMMON_CFG_OFFSET,
        region_length: VIRTIO_PCI_CFG_REGION_SIZE,
    };
    let notify = VirtioPciCap {
        offset: VIRTIO_PCI_CAP_NOTIFY_OFFSET,
        next: VIRTIO_PCI_CAP_ISR_OFFSET,
        cfg_type: VIRTIO_PCI_CAP_NOTIFY_CFG,
        bar: VIRTIO_PCI_BAR4,
        region_offset: VIRTIO_PCI_NOTIFY_CFG_OFFSET,
        region_length: VIRTIO_PCI_CFG_REGION_SIZE,
    };
    let isr = VirtioPciCap {
        offset: VIRTIO_PCI_CAP_ISR_OFFSET,
        next: VIRTIO_PCI_CAP_DEVICE_OFFSET,
        cfg_type: VIRTIO_PCI_CAP_ISR_CFG,
        bar: VIRTIO_PCI_BAR4,
        region_offset: VIRTIO_PCI_ISR_CFG_OFFSET,
        region_length: VIRTIO_PCI_CFG_REGION_SIZE,
    };
    let device = VirtioPciCap {
        offset: VIRTIO_PCI_CAP_DEVICE_OFFSET,
        next: msix_cap_offset,
        cfg_type: VIRTIO_PCI_CAP_DEVICE_CFG,
        bar: VIRTIO_PCI_BAR4,
        region_offset: VIRTIO_PCI_DEVICE_CFG_OFFSET,
        region_length: VIRTIO_PCI_CFG_REGION_SIZE,
    };

    let mut cap_bytes = Vec::with_capacity(68);
    push_bytes(&mut cap_bytes, common.offset, &common.bytes());
    let mut notify_bytes = [0u8; 20];
    notify_bytes[..16].copy_from_slice(&notify.bytes());
    notify_bytes[2] = 20;
    notify_bytes[16..20].copy_from_slice(&VIRTIO_PCI_NOTIFY_OFF_MULTIPLIER.to_le_bytes());
    push_bytes(&mut cap_bytes, notify.offset, &notify_bytes);
    push_bytes(&mut cap_bytes, isr.offset, &isr.bytes());
    push_bytes(&mut cap_bytes, device.offset, &device.bytes());

    VirtioCapabilityList {
        cap_ptr: VIRTIO_PCI_CAP_COMMON_OFFSET,
        cap_bytes,
    }
}

fn push_bytes(cap_bytes: &mut Vec<(u16, u8)>, offset: u8, bytes: &[u8]) {
    let mut register = u16::from(offset);
    for byte in bytes {
        cap_bytes.push((register, *byte));
        register += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        PcieEcam, CAP_ID_MSIX, REG_CAP_PTR, REG_COMMAND_STATUS, STATUS_CAP_LIST, VIRTIO_BLK_BDF,
    };
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct VirtioVendorCap {
        cfg_type: u8,
        bar: u8,
        offset: u32,
        length: u32,
        notify_off_multiplier: Option<u32>,
    }

    fn ecam_offset(bdf: (u8, u8, u8), reg: u16) -> u64 {
        let (bus, dev, func) = bdf;
        (u64::from(bus) << 20) | (u64::from(dev) << 15) | (u64::from(func) << 12) | u64::from(reg)
    }

    fn read_u8(ecam: &PcieEcam, bdf: (u8, u8, u8), reg: u16) -> u8 {
        u8::try_from(ecam.cfg_read(ecam_offset(bdf, reg), 1))
            .expect("single-byte config reads fit in u8")
    }

    fn read_u32(ecam: &PcieEcam, bdf: (u8, u8, u8), reg: u16) -> u32 {
        u32::try_from(ecam.cfg_read(ecam_offset(bdf, reg), 4))
            .expect("dword config reads fit in u32")
    }

    #[test]
    fn boot_media_endpoint_exposes_modern_virtio_pci_vendor_capability_list() {
        let ecam = PcieEcam::new();

        let status = ecam.cfg_read(ecam_offset(VIRTIO_BLK_BDF, REG_COMMAND_STATUS), 4) >> 16;
        assert_ne!(
            status & u64::from(STATUS_CAP_LIST),
            0,
            "virtio-blk endpoint must advertise a PCI capability list"
        );

        let mut cap = read_u8(&ecam, VIRTIO_BLK_BDF, REG_CAP_PTR);
        assert_ne!(cap, 0, "virtio-blk capability pointer must be nonzero");

        let mut caps = Vec::new();
        for _ in 0..8 {
            let cap_id = read_u8(&ecam, VIRTIO_BLK_BDF, u16::from(cap));
            if cap_id == CAP_ID_MSIX {
                break;
            }
            assert_eq!(
                cap_id, VIRTIO_PCI_CAP_VENDOR,
                "virtio transport capability must use PCI vendor capability id"
            );
            let cap_len = read_u8(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 2);
            let cfg_type = read_u8(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 3);
            let bar = read_u8(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 4);
            let offset = read_u32(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 8);
            let length = read_u32(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 12);
            let notify_off_multiplier = if cfg_type == VIRTIO_PCI_CAP_NOTIFY_CFG {
                assert_eq!(cap_len, 20, "notify capability must use the 20-byte layout");
                Some(read_u32(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 16))
            } else {
                assert_eq!(
                    cap_len, 16,
                    "non-notify virtio capability must use base layout"
                );
                None
            };
            caps.push(VirtioVendorCap {
                cfg_type,
                bar,
                offset,
                length,
                notify_off_multiplier,
            });

            cap = read_u8(&ecam, VIRTIO_BLK_BDF, u16::from(cap) + 1);
            if cap == 0 {
                break;
            }
        }

        assert_eq!(
            caps,
            vec![
                VirtioVendorCap {
                    cfg_type: VIRTIO_PCI_CAP_COMMON_CFG,
                    bar: VIRTIO_PCI_BAR4,
                    offset: VIRTIO_PCI_COMMON_CFG_OFFSET,
                    length: VIRTIO_PCI_CFG_REGION_SIZE,
                    notify_off_multiplier: None,
                },
                VirtioVendorCap {
                    cfg_type: VIRTIO_PCI_CAP_NOTIFY_CFG,
                    bar: VIRTIO_PCI_BAR4,
                    offset: VIRTIO_PCI_NOTIFY_CFG_OFFSET,
                    length: VIRTIO_PCI_CFG_REGION_SIZE,
                    notify_off_multiplier: Some(VIRTIO_PCI_NOTIFY_OFF_MULTIPLIER),
                },
                VirtioVendorCap {
                    cfg_type: VIRTIO_PCI_CAP_ISR_CFG,
                    bar: VIRTIO_PCI_BAR4,
                    offset: VIRTIO_PCI_ISR_CFG_OFFSET,
                    length: VIRTIO_PCI_CFG_REGION_SIZE,
                    notify_off_multiplier: None,
                },
                VirtioVendorCap {
                    cfg_type: VIRTIO_PCI_CAP_DEVICE_CFG,
                    bar: VIRTIO_PCI_BAR4,
                    offset: VIRTIO_PCI_DEVICE_CFG_OFFSET,
                    length: VIRTIO_PCI_CFG_REGION_SIZE,
                    notify_off_multiplier: None,
                },
            ]
        );
    }
}

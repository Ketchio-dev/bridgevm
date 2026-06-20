use crate::pcie::VIRTIO_BLK_BDF;

pub(super) const QEMU_VIRTIO_BLK_PCI_BOOTORDER: &[u8] = b"/pci@i0cf8/scsi@3/disk@0,0\n\0";

const _: () = assert!(VIRTIO_BLK_BDF.0 == 0);
const _: () = assert!(VIRTIO_BLK_BDF.1 == 3);
const _: () = assert!(VIRTIO_BLK_BDF.2 == 0);

pub(super) fn qemu_virtio_blk_pci_bootorder() -> Vec<u8> {
    QEMU_VIRTIO_BLK_PCI_BOOTORDER.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qemu_virtio_blk_pci_bootorder_uses_edk2_pci_scsi_path() {
        let bootorder = qemu_virtio_blk_pci_bootorder();

        assert_eq!(bootorder, QEMU_VIRTIO_BLK_PCI_BOOTORDER);
        assert!(bootorder.ends_with(b"\n\0"));
        assert!(bootorder.starts_with(b"/pci@i0cf8/scsi@3/"));
        assert!(bootorder
            .windows(b"/disk@0,0".len())
            .any(|window| window == b"/disk@0,0"));
        assert!(!bootorder.windows(4).any(|window| window == b"NVMe"));
    }
}

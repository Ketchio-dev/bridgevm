//! virtio MMIO and PCI-capability register map, and byte-lane register read/write math.

use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976;

pub(crate) const VERSION_MODERN: u32 = 2;

pub(crate) const DEVICE_ID_GPU: u32 = 16;

pub(crate) const VENDOR_ID_QEMU: u32 = 0x554d_4551;

pub(crate) const REG_MAGIC: u64 = 0x000;

pub(crate) const REG_VERSION: u64 = 0x004;

pub(crate) const REG_DEVICE_ID: u64 = 0x008;

pub(crate) const REG_VENDOR_ID: u64 = 0x00c;

pub(crate) const REG_DEVICE_FEATURES: u64 = 0x010;

pub(crate) const REG_DEVICE_FEATURES_SEL: u64 = 0x014;

pub(crate) const REG_DRIVER_FEATURES: u64 = 0x020;

pub(crate) const REG_DRIVER_FEATURES_SEL: u64 = 0x024;

pub(crate) const REG_QUEUE_SEL: u64 = 0x030;

pub(crate) const REG_QUEUE_NUM_MAX: u64 = 0x034;

pub(crate) const REG_QUEUE_NUM: u64 = 0x038;

pub(crate) const REG_QUEUE_READY: u64 = 0x044;

pub(crate) const REG_QUEUE_NOTIFY: u64 = 0x050;

pub(crate) const REG_INTERRUPT_STATUS: u64 = 0x060;

pub(crate) const REG_INTERRUPT_ACK: u64 = 0x064;

pub(crate) const REG_STATUS: u64 = 0x070;

pub(crate) const REG_QUEUE_DESC_LOW: u64 = 0x080;

pub(crate) const REG_QUEUE_DESC_HIGH: u64 = 0x084;

pub(crate) const REG_QUEUE_DRIVER_LOW: u64 = 0x090;

pub(crate) const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;

pub(crate) const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;

pub(crate) const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;

pub(crate) const REG_CONFIG_GENERATION: u64 = 0x0fc;

pub(crate) const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;

pub(crate) const PCI_ISR_CFG_OFFSET: u64 = 0x1000;

pub(crate) const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;

pub(crate) const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;

pub(crate) const PCI_CFG_REGION_SIZE: u64 = 0x1000;

pub(crate) const COMMON_DEVICE_FEATURE_SELECT: u64 = 0x00;

pub(crate) const COMMON_DEVICE_FEATURE: u64 = 0x04;

pub(crate) const COMMON_DRIVER_FEATURE_SELECT: u64 = 0x08;

pub(crate) const COMMON_DRIVER_FEATURE: u64 = 0x0c;

pub(crate) const COMMON_CONFIG_MSIX_VECTOR: u64 = 0x10;

pub(crate) const COMMON_NUM_QUEUES: u64 = 0x12;

pub(crate) const COMMON_DEVICE_STATUS: u64 = 0x14;

pub(crate) const COMMON_CONFIG_GENERATION: u64 = 0x15;

pub(crate) const COMMON_QUEUE_SELECT: u64 = 0x16;

pub(crate) const COMMON_QUEUE_SIZE: u64 = 0x18;

pub(crate) const COMMON_QUEUE_MSIX_VECTOR: u64 = 0x1a;

pub(crate) const COMMON_QUEUE_ENABLE: u64 = 0x1c;

pub(crate) const COMMON_QUEUE_NOTIFY_OFF: u64 = 0x1e;

pub(crate) const COMMON_QUEUE_DESC: u64 = 0x20;

pub(crate) const COMMON_QUEUE_DRIVER: u64 = 0x28;

pub(crate) const COMMON_QUEUE_DEVICE: u64 = 0x30;

pub(crate) const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

pub(crate) fn common_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_COMMON_CFG_OFFSET)
}

pub(crate) fn device_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_DEVICE_CFG_OFFSET)
}

pub(crate) fn notify_queue_index(offset: u64) -> Option<u16> {
    let rel = offset.checked_sub(PCI_NOTIFY_CFG_OFFSET)?;
    (rel < PCI_CFG_REGION_SIZE).then_some((rel / 4) as u16)
}

pub(crate) fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

pub(crate) fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

pub(crate) fn is_supported_common_access_size(size: u8) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

pub(crate) fn common_access_touches(base: u64, width: u8, offset: u64, size: u8) -> bool {
    let access_end = offset.saturating_add(u64::from(size));
    let field_end = base + u64::from(width);
    offset < field_end && base < access_end
}

pub(crate) fn common_access_touches_queue_field(offset: u64, size: u8) -> bool {
    [
        (COMMON_QUEUE_SIZE, 2),
        (COMMON_QUEUE_MSIX_VECTOR, 2),
        (COMMON_QUEUE_ENABLE, 2),
        (COMMON_QUEUE_DESC, 8),
        (COMMON_QUEUE_DRIVER, 8),
        (COMMON_QUEUE_DEVICE, 8),
    ]
    .iter()
    .any(|(base, width)| common_access_touches(*base, *width, offset, size))
}

pub(crate) fn read_common_register(
    base: u64,
    width: u8,
    value: u64,
    offset: u64,
    size: u8,
) -> Option<u64> {
    if !common_access_touches(base, width, offset, size) {
        return None;
    }
    let mut out = 0u64;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let byte = (value >> (field_byte * 8)) & 0xff;
        out |= byte << (u64::from(access_byte) * 8);
    }
    Some(mask_to_size(out, size))
}

pub(crate) fn write_common_register(
    current: u64,
    base: u64,
    width: u8,
    offset: u64,
    size: u8,
    value: u64,
) -> u64 {
    let mut out = current;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let shift = field_byte * 8;
        let byte = (value >> (u64::from(access_byte) * 8)) & 0xff;
        out = (out & !(0xff << shift)) | (byte << shift);
    }
    let bits = u64::from(width) * 8;
    if bits == 64 {
        out
    } else {
        out & ((1u64 << bits) - 1)
    }
}

pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn valid_msix_vector(vector: u16) -> u16 {
    if vector < VIRTIO_GPU_MSIX_VECTOR_COUNT || vector == VIRTIO_MSI_NO_VECTOR {
        vector
    } else {
        VIRTIO_MSI_NO_VECTOR
    }
}

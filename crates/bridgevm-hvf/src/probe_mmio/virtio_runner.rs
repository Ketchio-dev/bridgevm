//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

pub(crate) fn synthetic_block_byte(sector: u64, offset: u32) -> u8 {
    sector.wrapping_add(u64::from(offset)) as u8
}

pub(crate) fn seed_synthetic_virtio_block_read_request(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    memory.write_u32(
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS,
        VIRTIO_BLK_T_IN,
    )?;
    memory.write_u32(VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS + 4, 0)?;
    memory.write_u64(
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS + 8,
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR,
    )?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 1,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 0)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS,
        len: VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES,
        flags: VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
        next: 2,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 1)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 2)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 0)
}

pub(crate) fn seed_synthetic_virtio_block_write_request(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    memory.write_u32(
        VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS,
        VIRTIO_BLK_T_OUT,
    )?;
    memory.write_u32(VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS + 4, 0)?;
    memory.write_u64(
        VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS + 8,
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR,
    )?;
    let mut data = vec![0_u8; VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = 0xe0_u8.wrapping_add(index as u8);
    }
    memory.write_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS, &data)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 4,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 3)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS,
        len: VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 5,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 4)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 5)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 2)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 6, 3)
}

#[cfg(test)]
pub(crate) fn seed_synthetic_virtio_block_write_request_as_first(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    seed_synthetic_virtio_block_write_request(memory)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 3)
}

pub(crate) fn seed_synthetic_virtio_block_flush_request(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    memory.write_u32(
        VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS,
        VIRTIO_BLK_T_FLUSH,
    )?;
    memory.write_u32(VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS + 4, 0)?;
    memory.write_u64(
        VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS + 8,
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR,
    )?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 7,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 6)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 7)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 3)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 8, 6)
}

pub(crate) fn write_virtio_block_mmio_bus(
    bus: &mut MmioBus,
    block_base: u64,
    register: &'static str,
    offset: u64,
    value: u64,
) -> Result<(), VirtioBlockRequestError> {
    let expected = value & 0xffff_ffff;
    let action = bus.dispatch(MmioAccess::write(block_base + offset, value, 4));
    match action {
        MmioAction::WriteAccepted { value, .. } if value == expected => Ok(()),
        action => Err(VirtioBlockRequestError::UnexpectedMmioAction { register, action }),
    }
}

pub(crate) fn run_virtio_block_request_model(
) -> Result<VirtioBlockRequestModelProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let completion = block.complete_next_available_block_request(&mut memory)?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    Ok(VirtioBlockRequestModelProbe {
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        completed_via_device_bus: true,
        completed: true,
        descriptor_index: Some(completion.descriptor_index),
        request_type: Some(completion.request_type),
        sector: Some(completion.sector),
        data_bytes: Some(completion.data_bytes),
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: Some(memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0]),
        used_index: Some(completion.used_index),
        used_len: Some(used_len),
        interrupt_status: Some(completion.interrupt_status),
        blockers: Vec::new(),
    })
}

pub(crate) fn run_virtio_block_file_backing(
    disk_path: PathBuf,
) -> Result<VirtioBlockFileBackingProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let mut backend = FileBlockStorageBackend::open(&disk_path)?;
    let backing_kind = backend.kind();
    let completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let byte_offset = completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: completion.sector,
        })?;
    Ok(VirtioBlockFileBackingProbe {
        disk_path,
        backing_kind,
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        completed_via_device_bus: true,
        completed: true,
        descriptor_index: Some(completion.descriptor_index),
        request_type: Some(completion.request_type),
        sector: Some(completion.sector),
        byte_offset: Some(byte_offset),
        data_bytes: Some(completion.data_bytes),
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: Some(memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0]),
        used_index: Some(completion.used_index),
        used_len: Some(used_len),
        interrupt_status: Some(completion.interrupt_status),
        blockers: Vec::new(),
    })
}

pub(crate) fn run_virtio_block_writable_file_backing(
    disk_path: PathBuf,
) -> Result<VirtioBlockWritableFileBackingProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let mut backend = WritableHostFileBlockStorageBackend::open(&disk_path)?;
    let backing_kind = backend.kind();
    block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let initial_read_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?;

    seed_synthetic_virtio_block_write_request(&mut memory)?;
    let write_completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let write_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 16)?;
    let write_byte_offset = write_completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: write_completion.sector,
        })?;
    let write_data_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS, 8)?;
    let write_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS, 1)?[0];

    seed_synthetic_virtio_block_flush_request(&mut memory)?;
    let flush_completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let flush_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 24)?;
    let flush_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS, 1)?[0];
    let interrupt_status = flush_completion.interrupt_status;
    drop(backend);

    let mut persisted_data_prefix = vec![0_u8; 8];
    let mut reopened = FileBlockStorageBackend::open(&disk_path)?;
    reopened.read_exact_at(write_byte_offset, &mut persisted_data_prefix)?;

    Ok(VirtioBlockWritableFileBackingProbe {
        disk_path,
        backing_kind,
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        initial_read_prefix,
        write_completed: true,
        write_request_type: Some(write_completion.request_type),
        write_sector: Some(write_completion.sector),
        write_byte_offset: Some(write_byte_offset),
        write_data_bytes: Some(write_completion.data_bytes),
        write_data_prefix,
        write_status: Some(write_status),
        write_used_index: Some(write_completion.used_index),
        write_used_len: Some(write_used_len),
        flush_completed: true,
        flush_request_type: Some(flush_completion.request_type),
        flush_status: Some(flush_status),
        flush_used_index: Some(flush_completion.used_index),
        flush_used_len: Some(flush_used_len),
        persisted_data_prefix,
        interrupt_status: Some(interrupt_status),
        blockers: Vec::new(),
    })
}

pub(crate) fn run_virtio_block_iso_backing(
    iso_path: PathBuf,
) -> Result<VirtioBlockIsoBackingProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let mut backend = ReadOnlyIsoBlockStorageBackend::open(&iso_path)?;
    let backing_kind = backend.kind();
    let completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let byte_offset = completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: completion.sector,
        })?;
    seed_synthetic_virtio_block_write_request(&mut memory)?;
    let write_completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let write_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 16)?;
    Ok(VirtioBlockIsoBackingProbe {
        iso_path,
        backing_kind,
        media_mode: "read-only",
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        completed_via_device_bus: true,
        completed: true,
        descriptor_index: Some(completion.descriptor_index),
        request_type: Some(completion.request_type),
        sector: Some(completion.sector),
        byte_offset: Some(byte_offset),
        data_bytes: Some(completion.data_bytes),
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: Some(memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0]),
        used_index: Some(completion.used_index),
        used_len: Some(used_len),
        interrupt_status: Some(completion.interrupt_status),
        readonly_write_rejected: write_completion.status == VIRTIO_BLK_S_IOERR,
        readonly_write_status: Some(write_completion.status),
        readonly_write_used_index: Some(write_completion.used_index),
        readonly_write_used_len: Some(write_used_len),
        readonly_write_interrupt_status: Some(write_completion.interrupt_status),
        blockers: Vec::new(),
    })
}

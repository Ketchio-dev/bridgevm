//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_GUEST;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_HOST3D;

#[test]
fn resource_transfer_flush_presents_pixels_to_scanout() {
    let mut dev = VirtioPciGpu::new(4, 3);
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let req = 0x4000_4000;
    let resp = 0x4000_5000;
    let backing = 0x4000_8000;
    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);

    let mut backing_bytes = vec![0u8; 4 * 3 * 4];
    backing_bytes[4 * 4..4 * 4 + 4].copy_from_slice(&[0x33, 0x22, 0x11, 0xaa]);
    backing_bytes[5 * 4..5 * 4 + 4].copy_from_slice(&[0x66, 0x55, 0x44, 0xbb]);
    mem.write(backing, &backing_bytes);

    let mut chains = Vec::new();
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    create.extend_from_slice(&1u32.to_le_bytes());
    create.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
    create.extend_from_slice(&4u32.to_le_bytes());
    create.extend_from_slice(&3u32.to_le_bytes());
    chains.push(create);

    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&backing.to_le_bytes());
    attach.extend_from_slice(&(backing_bytes.len() as u32).to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    chains.push(attach);

    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    push_rect(
        &mut set_scanout,
        Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 3,
        },
    );
    set_scanout.extend_from_slice(&0u32.to_le_bytes());
    set_scanout.extend_from_slice(&1u32.to_le_bytes());
    chains.push(set_scanout);

    let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
    push_rect(
        &mut transfer,
        Rect {
            x: 0,
            y: 1,
            width: 2,
            height: 1,
        },
    );
    // offset locates the box top-left (0, 1) in the backing: y*stride =
    // 1 * (width 4 * 4bpp) = 16, matching a full-surface backing where the
    // guest points offset at the dirty region's origin (Convention B).
    transfer.extend_from_slice(&16u64.to_le_bytes());
    transfer.extend_from_slice(&1u32.to_le_bytes());
    transfer.extend_from_slice(&0u32.to_le_bytes());
    chains.push(transfer);

    let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
    push_rect(
        &mut flush,
        Rect {
            x: 0,
            y: 1,
            width: 2,
            height: 1,
        },
    );
    flush.extend_from_slice(&1u32.to_le_bytes());
    flush.extend_from_slice(&0u32.to_le_bytes());
    chains.push(flush);

    for (i, request) in chains.iter().enumerate() {
        let req_addr = req + (i as u64) * 0x100;
        let resp_addr = resp + (i as u64) * 0x100;
        mem.write(req_addr, request);
        write_desc(
            &mut mem,
            desc,
            (i * 2) as u16,
            req_addr,
            request.len() as u32,
            DESC_F_NEXT,
            (i * 2 + 1) as u16,
        );
        write_desc(
            &mut mem,
            desc,
            (i * 2 + 1) as u16,
            resp_addr,
            24,
            DESC_F_WRITE,
            0,
        );
        mem.write(avail + 4 + (i as u64) * 2, &((i * 2) as u16).to_le_bytes());
    }
    mem.write(avail + 2, &(chains.len() as u16).to_le_bytes());
    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        chains.len() as u16
    );
    let scanout = dev.scanout().unwrap();
    let row1 = (scanout.stride as usize)..(scanout.stride as usize + 8);
    assert_eq!(
        &scanout.bytes[row1],
        &[0x33, 0x22, 0x11, 0, 0x66, 0x55, 0x44, 0]
    );
}

#[test]
fn resource_transfer_split_backing_row_falls_back_to_pixel_reads() {
    let mut dev = VirtioPciGpu::new(2, 1);
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let backing_a = 0x4000_8000;
    let backing_b = 0x4000_9000;
    mem.write(backing_a, &[0x11, 0x22, 0x33, 0xff]);
    mem.write(backing_b, &[0x44, 0x55, 0x66, 0xee]);

    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    create.extend_from_slice(&1u32.to_le_bytes());
    create.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
    create.extend_from_slice(&2u32.to_le_bytes());
    create.extend_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&2u32.to_le_bytes());
    attach.extend_from_slice(&backing_a.to_le_bytes());
    attach.extend_from_slice(&4u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    attach.extend_from_slice(&backing_b.to_le_bytes());
    attach.extend_from_slice(&4u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    push_rect(
        &mut set_scanout,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );
    set_scanout.extend_from_slice(&0u32.to_le_bytes());
    set_scanout.extend_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
    push_rect(
        &mut transfer,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );
    transfer.extend_from_slice(&0u64.to_le_bytes());
    transfer.extend_from_slice(&1u32.to_le_bytes());
    transfer.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &transfer, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let flush = flush_req(
        1,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert_eq!(
        &dev.scanout().unwrap().bytes[0..8],
        &[0x11, 0x22, 0x33, 0, 0x44, 0x55, 0x66, 0]
    );
}

#[test]
fn set_scanout_blob_guest_flush_presents_pixels_with_stride_and_offset() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let backing = 0x4000_8000;
    let mut backing_bytes = vec![0u8; 64];
    backing_bytes[4..8].copy_from_slice(&[0x10, 0x20, 0x30, 0xff]);
    backing_bytes[8..12].copy_from_slice(&[0x40, 0x50, 0x60, 0xee]);
    backing_bytes[20..24].copy_from_slice(&[0x70, 0x80, 0x90, 0xdd]);
    backing_bytes[24..28].copy_from_slice(&[0xa0, 0xb0, 0xc0, 0xcc]);
    mem.write(backing, &backing_bytes);

    let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_GUEST, 64, &[(backing, 64)]);
    let resp = submit_control(&mut dev, &mut mem, &create, 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

    let set_scanout = set_scanout_blob_req(7, 2, 2, FORMAT_B8G8R8A8_UNORM, 16, 4);
    let resp = submit_control(&mut dev, &mut mem, &set_scanout, 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

    let flush = flush_req(
        7,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
    );
    let resp = submit_control(&mut dev, &mut mem, &flush, 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

    let scanout = dev.scanout().unwrap();
    assert_eq!(
        &scanout.bytes[0..8],
        &[0x10, 0x20, 0x30, 0, 0x40, 0x50, 0x60, 0]
    );
    let row1 = scanout.stride as usize;
    assert_eq!(
        &scanout.bytes[row1..row1 + 8],
        &[0x70, 0x80, 0x90, 0, 0xa0, 0xb0, 0xc0, 0]
    );
}

#[test]
fn set_scanout_blob_guest_flush_reuses_row_scratch() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let backing = 0x4000_8000;
    let backing_bytes = [
        0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xee, 0x70, 0x80, 0x90, 0xdd, 0xa0, 0xb0, 0xc0,
        0xcc,
    ];
    mem.write(backing, &backing_bytes);

    let create = create_blob_req(17, VIRTIO_GPU_BLOB_MEM_GUEST, 16, &[(backing, 16)]);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let set_scanout = set_scanout_blob_req(17, 2, 2, FORMAT_B8G8R8A8_UNORM, 8, 0);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let flush = flush_req(
        17,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
    );

    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert!(dev.gpu.blob_row_scratch.is_empty());
    assert!(dev.gpu.blob_row_scratch.capacity() >= 8);
    let row_scratch = (
        dev.gpu.blob_row_scratch.as_ptr(),
        dev.gpu.blob_row_scratch.capacity(),
    );

    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert!(dev.gpu.blob_row_scratch.is_empty());
    assert_eq!(
        (
            dev.gpu.blob_row_scratch.as_ptr(),
            dev.gpu.blob_row_scratch.capacity()
        ),
        row_scratch
    );
}

#[test]
fn set_scanout_blob_guest_split_backing_row_falls_back_to_pixel_reads() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let backing_a = 0x4000_8000;
    let backing_b = 0x4000_9000;
    mem.write(backing_a, &[0x12, 0x23, 0x34, 0xff]);
    mem.write(backing_b, &[0x45, 0x56, 0x67, 0xee]);

    let create = create_blob_req(
        7,
        VIRTIO_GPU_BLOB_MEM_GUEST,
        8,
        &[(backing_a, 4), (backing_b, 4)],
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let set_scanout = set_scanout_blob_req(7, 2, 1, FORMAT_B8G8R8A8_UNORM, 8, 0);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let flush = flush_req(
        7,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert_eq!(
        &dev.scanout().unwrap().bytes[0..8],
        &[0x12, 0x23, 0x34, 0, 0x45, 0x56, 0x67, 0]
    );
}

#[test]
fn set_scanout_blob_host3d_flush_presents_pixels_from_mock_mapping() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let mut host_pixels = vec![0u8; 32];
    host_pixels[0..4].copy_from_slice(&[0x11, 0x22, 0x33, 0xff]);
    host_pixels[4..8].copy_from_slice(&[0x44, 0x55, 0x66, 0xee]);
    backend.lock().unwrap().mapped.insert(
        9,
        virtio_gpu_3d::MappedBlob {
            host_ptr: host_pixels.as_mut_ptr(),
            size: host_pixels.len(),
            map_info: 0,
        },
    );

    let create = create_blob_req(9, VIRTIO_GPU_BLOB_MEM_HOST3D, 32, &[]);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let set_scanout = set_scanout_blob_req(9, 2, 1, FORMAT_B8G8R8A8_UNORM, 8, 0);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let flush = flush_req(
        9,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert_eq!(
        &dev.scanout().unwrap().bytes[0..8],
        &[0x11, 0x22, 0x33, 0, 0x44, 0x55, 0x66, 0]
    );
    assert!(backend.lock().unwrap().unmapped.is_empty());
}

#[test]
fn set_scanout_blob_unknown_resource_errors() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let set_scanout = set_scanout_blob_req(99, 2, 1, FORMAT_B8G8R8A8_UNORM, 8, 0);
    let resp = submit_control(&mut dev, &mut mem, &set_scanout, 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_ERR_UNSPEC));
}

#[test]
fn set_scanout_blob_resource_zero_unbinds() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let mut host_pixels = vec![0u8; 16];
    backend.lock().unwrap().mapped.insert(
        10,
        virtio_gpu_3d::MappedBlob {
            host_ptr: host_pixels.as_mut_ptr(),
            size: host_pixels.len(),
            map_info: 0,
        },
    );
    let create = create_blob_req(10, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
    let set_scanout = set_scanout_blob_req(10, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
    let unbind = set_scanout_blob_req(0, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);

    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert!(dev.scanout().is_some());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &unbind, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert!(dev.scanout().is_none());
    assert_eq!(backend.lock().unwrap().unmapped, vec![10]);
}

#[test]
fn two_d_scanout_still_works_after_blob_unbind() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let blob_backing = 0x4000_7000;
    mem.write(blob_backing, &[0u8; 16]);
    let create_blob = create_blob_req(12, VIRTIO_GPU_BLOB_MEM_GUEST, 16, &[(blob_backing, 16)]);
    let set_blob = set_scanout_blob_req(12, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
    let unbind = set_scanout_blob_req(0, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create_blob, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_blob, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &unbind, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let backing = 0x4000_8000;
    mem.write(backing, &[0x21, 0x32, 0x43, 0xff]);
    let mut create_2d = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    create_2d.extend_from_slice(&1u32.to_le_bytes());
    create_2d.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
    create_2d.extend_from_slice(&1u32.to_le_bytes());
    create_2d.extend_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create_2d, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&backing.to_le_bytes());
    attach.extend_from_slice(&4u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let mut set_2d = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    push_rect(
        &mut set_2d,
        Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
    );
    set_2d.extend_from_slice(&0u32.to_le_bytes());
    set_2d.extend_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_2d, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
    push_rect(
        &mut transfer,
        Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
    );
    transfer.extend_from_slice(&0u64.to_le_bytes());
    transfer.extend_from_slice(&1u32.to_le_bytes());
    transfer.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &transfer, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let flush = flush_req(
        1,
        Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(&dev.scanout().unwrap().bytes[0..4], &[0x21, 0x32, 0x43, 0]);
}

#[test]
fn reset_clears_blob_scanout_and_unmaps() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let mut host_pixels = vec![0u8; 16];
    backend.lock().unwrap().mapped.insert(
        13,
        virtio_gpu_3d::MappedBlob {
            host_ptr: host_pixels.as_mut_ptr(),
            size: host_pixels.len(),
            map_info: 0,
        },
    );
    let create = create_blob_req(13, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
    let set_scanout = set_scanout_blob_req(13, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    dev.gpu.scanout[0] = 0xff;
    let scanout_capacity = dev.gpu.scanout.capacity();
    let scanout_ptr = dev.gpu.scanout.as_ptr();

    dev.reset_runtime_state();

    assert!(dev.scanout().is_none());
    assert_eq!(backend.lock().unwrap().unmapped, vec![13]);
    assert!(!dev.stats().scanout_active);
    assert_eq!(dev.gpu.scanout.capacity(), scanout_capacity);
    assert_eq!(dev.gpu.scanout.as_ptr(), scanout_ptr);
    assert!(dev.gpu.scanout.iter().all(|byte| *byte == 0));
}

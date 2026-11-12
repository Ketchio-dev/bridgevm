//! Guest RESOURCE_CREATE_2D sizes are bounded by a host-memory budget.

use super::super::*;
use super::helpers::*;
use crate::virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY;

fn create(dev: &mut VirtioPciGpu, mem: &mut TestMem, id: u32, width: u32, height: u32) -> u32 {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    for value in [id, FORMAT_B8G8R8A8_UNORM, width, height] {
        req.extend_from_slice(&value.to_le_bytes());
    }
    read_le_u32(&submit_control(dev, mem, &req, 24), 0).unwrap()
}

fn unref(dev: &mut VirtioPciGpu, mem: &mut TestMem, id: u32) -> u32 {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNREF);
    req.extend_from_slice(&id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    read_le_u32(&submit_control(dev, mem, &req, 24), 0).unwrap()
}

#[test]
fn incomplete_transfer_preserves_readable_pixels() {
    struct HoleMem;
    impl crate::fwcfg::GuestMemoryMut for HoleMem {
        fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
            panic!("use read_into")
        }
        fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
            false
        }
        fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
            if gpa == 0 && dst.len() <= 4 {
                dst.fill(0x11);
                true
            } else if gpa == 8 && dst.len() <= 4 {
                dst.fill(0x33);
                true
            } else {
                false
            }
        }
    }
    let mut resource = GpuResource {
        format: FORMAT_B8G8R8A8_UNORM,
        width: 4,
        height: 1,
        host_pixels: vec![0xaa; 16],
        backing: vec![BackingEntry { addr: 0, len: 12 }],
        backing_len: 12,
    };
    resource::copy_backing_to_resource(
        &HoleMem,
        &mut resource,
        Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 1,
        },
        0,
    );
    assert_eq!(
        resource.host_pixels,
        [vec![0x11; 4], vec![0xaa; 4], vec![0x33; 4], vec![0xaa; 4]].concat()
    );
}

#[test]
fn bounds_regression_uncovered_transfer_work_is_linear() {
    for unlocked in [false, true] {
        let mut dev = VirtioPciGpu::new(16, 1);
        dev.set_unlocked_copy(unlocked);
        let mut mem = TestMem::new(0x4000_0000, 0x10000);
        dev.gpu.resources.insert(
            1,
            GpuResource {
                format: FORMAT_B8G8R8A8_UNORM,
                width: 16,
                height: 1,
                host_pixels: vec![0; 64],
                backing: vec![BackingEntry { addr: 0, len: 0 }; 4096],
                backing_len: 0,
            },
        );
        let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
        push_rect(
            &mut transfer,
            Rect {
                x: 0,
                y: 0,
                width: 16,
                height: 1,
            },
        );
        transfer.extend_from_slice(&0u64.to_le_bytes());
        transfer.extend_from_slice(&1u32.to_le_bytes());
        transfer.extend_from_slice(&0u32.to_le_bytes());
        backing_copy_cursor::take_entry_visits();
        let response = if unlocked {
            setup_queue(
                &mut dev,
                &mut mem,
                0,
                0x4000_1000,
                0x4000_2000,
                0x4000_3000,
                0,
            );
            mem.write(0x4000_4000, &transfer);
            write_desc(
                &mut mem,
                0x4000_1000,
                0,
                0x4000_4000,
                transfer.len() as u32,
                DESC_F_NEXT,
                1,
            );
            write_desc(&mut mem, 0x4000_1000, 1, 0x4000_5000, 24, DESC_F_WRITE, 0);
            mem.write(0x4000_2002, &1u16.to_le_bytes());
            mem.write(0x4000_2004, &0u16.to_le_bytes());
            pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);
            let mut copy = dev.take_copy().unwrap();
            copy.run(&mem);
            dev.finish_copy(copy, &mut mem);
            assert_eq!(mem.read(0x4000_3002, 2), 1u16.to_le_bytes());
            mem.read(0x4000_5000, 24)
        } else {
            submit_control(&mut dev, &mut mem, &transfer, 24)
        };
        let visits = backing_copy_cursor::take_entry_visits();
        // No covered bytes: the cached length avoids even a row-list pass.
        assert_eq!(visits, 0, "uncovered backing-entry visits");
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    }
}

#[test]
fn bounds_regression_backing_counts_toward_hostmem() {
    let mut dev = VirtioPciGpu::new(1, 1);
    let mem = TestMem::new(0x4000_0000, 0x10000);
    let mut response = Vec::new();
    // Leave room for one backing entry, but not two. The large allocation is
    // pixels, not guest memory, and remains strictly below the existing cap.
    dev.gpu.resources.insert(
        1,
        GpuResource {
            format: FORMAT_B8G8R8A8_UNORM,
            width: 1,
            height: 1,
            host_pixels: vec![0; resource::MAX_2D_HOSTMEM - 32],
            backing: vec![BackingEntry { addr: 0, len: 4 }],
            backing_len: 4,
        },
    );
    let previous = dev.gpu.resources[&1].backing.clone();
    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&2u32.to_le_bytes());
    attach.resize(64, 0);
    dev.gpu
        .attach_backing_into(&mem, &attach, None, &mut response);
    assert_eq!(
        read_le_u32(&response, 0),
        Some(VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY)
    );
    assert_eq!(dev.gpu.resources[&1].backing, previous);
    // Creation must count retained metadata on other resources as well.
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    for value in [2, FORMAT_B8G8R8A8_UNORM, 2, 2] {
        create.extend_from_slice(&value.to_le_bytes());
    }
    dev.gpu
        .resource_create_2d_into(&create, None, &mut response);
    assert_eq!(
        read_le_u32(&response, 0),
        Some(VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY)
    );
}

#[test]
fn oversized_create_is_refused_without_allocating() {
    let mut dev = VirtioPciGpu::new(64, 64);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    // 0x403c1a21 x 0x77943681 x 4 bytes fits in u64 but not in host memory.
    let oom = VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY;
    assert_eq!(create(&mut dev, &mut mem, 1, 0x403c_1a21, 0x7794_3681), oom);
    assert_eq!(create(&mut dev, &mut mem, 1, u32::MAX, u32::MAX), oom);
    // 8192 x 8192 x 4 is exactly the 256 MiB budget, which must stay below it.
    assert_eq!(create(&mut dev, &mut mem, 1, 8192, 8192), oom);
    assert!(dev.gpu.resources.is_empty());
}

#[test]
fn budget_is_cumulative_released_by_unref_and_replacement() {
    let mut dev = VirtioPciGpu::new(64, 64);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let ok = VIRTIO_GPU_RESP_OK_NODATA;
    let oom = VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY;
    // Two 7680x4320 resources (~127 MiB each) fit; a third does not.
    assert_eq!(create(&mut dev, &mut mem, 1, 7680, 4320), ok);
    assert_eq!(create(&mut dev, &mut mem, 2, 7680, 4320), ok);
    assert_eq!(create(&mut dev, &mut mem, 3, 7680, 4320), oom);
    // Re-creating an existing id replaces it rather than adding to the total.
    assert_eq!(create(&mut dev, &mut mem, 2, 7680, 4320), ok);
    assert_eq!(unref(&mut dev, &mut mem, 1), ok);
    assert_eq!(create(&mut dev, &mut mem, 3, 7680, 4320), ok);
    assert_eq!(
        dev.gpu.resources.keys().copied().collect::<Vec<_>>(),
        [2, 3]
    );
}

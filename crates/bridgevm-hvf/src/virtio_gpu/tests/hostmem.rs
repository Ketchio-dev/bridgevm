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

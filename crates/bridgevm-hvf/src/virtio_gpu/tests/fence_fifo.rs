//! FIFO fence retirement preserves delivery and retained-response order.

use super::super::*;
use super::helpers::*;
use crate::virtio_gpu_3d::CompletedFence;

const USED_GPA: u64 = 0x4000_1000;

fn pending_response(
    queue: VirtioGpuQueue,
    head: u16,
    response_gpa: u64,
    response_byte: u8,
    fence: CompletedFence,
) -> PendingFencedResponse {
    PendingFencedResponse {
        queue_index: 0,
        queue,
        head,
        descs: vec![Descriptor {
            addr: response_gpa,
            len: 4,
            flags: DESC_F_WRITE,
            next: 0,
        }],
        response: vec![response_byte; 4],
        fence,
    }
}

fn used_head(mem: &TestMem, index: u64) -> u32 {
    u32::from_le_bytes(mem.read(USED_GPA + 4 + index * 8, 4).try_into().unwrap())
}

#[test]
fn fence_retirement_delivers_ready_entries_and_preserves_unready_fifo_order() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let mut queue = VirtioGpuQueue::new(0);
    queue.size = 8;
    queue.ready = true;
    queue.device = USED_GPA;

    let entries = [
        (
            10,
            0x4000_2000,
            0xa1,
            CompletedFence {
                ctx_id: 1,
                ring_idx: 0,
                fence_id: 1,
            },
        ),
        (
            11,
            0x4000_2100,
            0xb2,
            CompletedFence {
                ctx_id: 2,
                ring_idx: 0,
                fence_id: 5,
            },
        ),
        (
            12,
            0x4000_2200,
            0xc3,
            CompletedFence {
                ctx_id: 1,
                ring_idx: 0,
                fence_id: 2,
            },
        ),
        (
            13,
            0x4000_2300,
            0xd4,
            CompletedFence {
                ctx_id: 3,
                ring_idx: 0,
                fence_id: 1,
            },
        ),
    ];
    for (head, response_gpa, response_byte, fence) in entries {
        dev.gpu.pending_fenced.push_back(pending_response(
            queue,
            head,
            response_gpa,
            response_byte,
            fence,
        ));
    }
    let pending_capacity = dev.gpu.pending_fenced.capacity();

    backend.lock().unwrap().completed.push(CompletedFence {
        ctx_id: 1,
        ring_idx: 0,
        fence_id: 2,
    });
    dev.drain_completed_fences(&mut mem);

    assert_eq!(
        dev.gpu
            .pending_fenced
            .iter()
            .map(|pending| (pending.fence.ctx_id, pending.fence.fence_id))
            .collect::<Vec<_>>(),
        vec![(2, 5), (3, 1)]
    );
    assert_eq!(dev.gpu.pending_fenced.capacity(), pending_capacity);
    assert_eq!(
        u16::from_le_bytes(mem.read(USED_GPA + 2, 2).try_into().unwrap()),
        2
    );
    assert_eq!((used_head(&mem, 0), used_head(&mem, 1)), (10, 12));
    assert_eq!(mem.read(0x4000_2000, 4), vec![0xa1; 4]);
    assert_eq!(mem.read(0x4000_2200, 4), vec![0xc3; 4]);
    assert_eq!(mem.read(0x4000_2100, 4), vec![0; 4]);
    assert_eq!(mem.read(0x4000_2300, 4), vec![0; 4]);
    assert!(dev.gpu.descriptor_scratch.capacity() >= 1);
    assert!(dev.gpu.response_scratch.capacity() >= 4);
    assert_eq!(dev.gpu.parked_descriptor_scratch.len(), 1);
    assert_eq!(dev.gpu.parked_response_scratch.len(), 1);

    backend.lock().unwrap().completed.extend([
        CompletedFence {
            ctx_id: 2,
            ring_idx: 0,
            fence_id: 5,
        },
        CompletedFence {
            ctx_id: 3,
            ring_idx: 0,
            fence_id: 1,
        },
    ]);
    dev.drain_completed_fences(&mut mem);

    assert!(dev.gpu.pending_fenced.is_empty());
    assert_eq!(
        u16::from_le_bytes(mem.read(USED_GPA + 2, 2).try_into().unwrap()),
        4
    );
    assert_eq!((used_head(&mem, 2), used_head(&mem, 3)), (11, 13));
    assert_eq!(mem.read(0x4000_2100, 4), vec![0xb2; 4]);
    assert_eq!(mem.read(0x4000_2300, 4), vec![0xd4; 4]);
}

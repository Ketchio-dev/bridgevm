use super::super::*;
use super::helpers::*;
use std::time::{Duration, Instant};

const PACED_QUEUE_SIZE: usize = 16;

#[test]
fn vblank_parking_stops_consuming_once_the_queue_size_is_in_flight() {
    let (mut dev, _backend) = dev_with_mock();
    let interval = Duration::from_millis(8);
    dev.set_vblank_interval(interval);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let request = submit_3d_req(0, &[]);
    let submit = |dev: &mut VirtioPciGpu, mem: &mut TestMem| {
        submit_control_readable_descs_at(
            dev,
            mem,
            &[&request],
            24,
            0x4000_1000,
            0x4000_4000,
            0x4000_9000,
        )
    };

    // A driver that reuses one head without waiting for its completion asks
    // the device to park more chains than the ring can hold.
    for _ in 0..PACED_QUEUE_SIZE + 4 {
        submit(&mut dev, &mut mem);
    }
    assert_eq!(dev.gpu.pending_vblank.len(), PACED_QUEUE_SIZE);
    assert_eq!(
        dev.stats().queues[0].last_avail_idx,
        PACED_QUEUE_SIZE as u16
    );

    // Retiring one parked response frees one slot for the next submission.
    dev.gpu.drain_host_vblank_at(&mut mem, Instant::now());
    assert_eq!(dev.gpu.pending_vblank.len(), PACED_QUEUE_SIZE - 1);
    submit(&mut dev, &mut mem);
    assert_eq!(dev.gpu.pending_vblank.len(), PACED_QUEUE_SIZE);
}

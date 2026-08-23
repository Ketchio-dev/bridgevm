use super::helpers::{deferred_scanout_dev, flush_res_31, submit_control};

#[test]
fn same_resource_flush_cannot_rearm_the_fresh_guard_forever() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout(); // consume the one-exit fresh guard

    // Windows can flush again before the next per-exit drain. Coalescing that
    // update must preserve the already-consumed guard or continuous frames
    // starve presentation until the guest happens to leave a flush gap.
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();

    assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);
}

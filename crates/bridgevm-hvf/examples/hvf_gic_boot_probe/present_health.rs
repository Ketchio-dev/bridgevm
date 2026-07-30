/// Prints the end-of-run 3D presentation churn health summary.
pub(crate) fn print_present_health(platform: &bridgevm_hvf::platform_virt::VirtPlatform) {
    let Some(stats) = platform.virtio_gpu_stats() else {
        return;
    };
    let create3d = stats.resource_create_3d_count;
    let flush = stats.scanout_3d_flushes;
    if flush == 0 {
        println!(
            "present health create3d={create3d} flush={flush} ratio=n/a healthy=false threshold=0.10"
        );
        return;
    }
    let ratio = create3d as f64 / flush as f64;
    println!(
        "present health create3d={create3d} flush={flush} ratio={ratio:.4} healthy={} threshold=0.10",
        ratio <= 0.10
    );
}

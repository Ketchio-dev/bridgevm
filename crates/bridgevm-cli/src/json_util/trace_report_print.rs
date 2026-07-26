//! Text rendering for the virtio-gpu JSONL analyzer.

use super::*;

pub(crate) fn print_virtio_gpu_trace_report(
    path: &Path,
    protocol: VirtioGpuTraceProtocolChoice,
    report: &VirtioGpuTraceReport,
    blockers: &[String],
) {
    println!("BridgeVM HVF virtio-gpu trace report");
    println!("Trace: {}", path.display());
    println!("Requested protocol: {}", protocol.label());
    println!("Selected protocol: {}", report.selected_protocol(protocol));
    println!("Non-empty lines: {}", report.lines);
    println!("Parsed events: {}", report.events);
    println!("Invalid lines: {}", report.invalid_lines.len());
    println!("Device initialized: {}", report.device_init);
    println!("3D backend attached: {}", report.has_3d_backend());
    println!(
        "Device feature word0: {}",
        hex_option(report.device_features_word0)
    );
    println!(
        "Device feature word1: {}",
        hex_option(report.device_features_word1)
    );
    println!(
        "Driver feature word0: {}",
        hex_option(report.driver_features_word0)
    );
    println!(
        "Driver feature word1: {}",
        hex_option(report.driver_features_word1)
    );
    println!(
        "VENUS feature set accepted: {}",
        report.accepted_venus_features()
    );
    println!(
        "VIRTIO_F_VERSION_1 accepted: {}",
        report.accepted_version_1()
    );
    println!("Queue notify observed: {}", report.queue_notify);
    println!("GET_CAPSET_INFO OK: {}", report.capset_info_ok);
    println!(
        "GET_CAPSET_INFO VIRGL/VIRGL2 id 1/2: {}",
        report.virgl_capset_info_ok
    );
    println!(
        "GET_CAPSET_INFO VENUS id 4: {}",
        report.venus_capset_info_ok
    );
    println!("GET_CAPSET OK: {}", report.capset_ok);
    println!("GET_CAPSET VIRGL/VIRGL2 id 1/2: {}", report.virgl_capset_ok);
    println!("GET_CAPSET VENUS id 4: {}", report.venus_capset_ok);
    println!("RESOURCE_CREATE_3D OK: {}", report.resource_create_3d_ok);
    println!(
        "RESOURCE_ATTACH_BACKING OK: {}",
        report.resource_attach_backing_ok
    );
    println!("RESOURCE_CREATE_BLOB OK: {}", report.blob_create_ok);
    println!("CTX_CREATE OK: {}", report.ctx_create_ok);
    println!(
        "CTX_CREATE VIRGL/VIRGL2 context_init: {}",
        report.virgl_ctx_create_ok
    );
    println!(
        "CTX_CREATE VENUS context_init: {}",
        report.venus_ctx_create_ok
    );
    println!("SUBMIT_3D OK: {}", report.submit_3d_ok);
    println!("SUBMIT_3D non-empty: {}", report.submit_3d_nonzero_ok);
    println!(
        "Command duration p50/p95/p99 us: {:.3} / {:.3} / {:.3}",
        report.command_percentile_us(50, 100),
        report.command_percentile_us(95, 100),
        report.command_percentile_us(99, 100)
    );
    for name in ["SUBMIT_3D", "RESOURCE_FLUSH"] {
        println!(
            "{name} duration p50/p95/p99 us: {:.3} / {:.3} / {:.3}",
            report.named_command_percentile_us(name, 50, 100),
            report.named_command_percentile_us(name, 95, 100),
            report.named_command_percentile_us(name, 99, 100)
        );
    }
    if report.submit_3d_failures.is_empty() {
        println!("SUBMIT_3D failure groups: none");
    } else {
        println!(
            "SUBMIT_3D failure groups: {}",
            report.submit_3d_failures.len()
        );
        for (failure, count) in &report.submit_3d_failures {
            let command_id = option_label(failure.command_id);
            let command_name = failure
                .command_id
                .map(virgl_command_name)
                .unwrap_or("UNKNOWN");
            println!(
                "- count={count} response={} renderer_status={} command={command_id}({command_name}) resource={} found={} backed={}",
                failure.response,
                option_label(failure.renderer_status),
                option_label(failure.resource_id),
                option_bool_label(failure.resource_found),
                option_bool_label(failure.resource_backed)
            );
        }
    }
    println!("Fenced command observed: {}", report.fenced_command);
    println!("Fence create observed: {}", report.fence_create);
    println!(
        "Backend-parked fence observed: {}",
        report.backend_fence_parked
    );
    println!("Fence complete observed: {}", report.fence_complete);
    println!("Fence deliver observed: {}", report.fence_deliver);
    println!("Scanout readbacks: {}", report.scanout_readbacks);
    println!(
        "Scanout throttled flushes: {}",
        report.scanout_readback_throttled
    );
    println!("Scanout readback bytes: {}", report.scanout_readback_bytes);
    println!(
        "Scanout readback duration ns: {}",
        report.scanout_readback_nanoseconds
    );
    println!(
        "Scanout readback average us: {:.3}",
        report.scanout_readback_average_us()
    );
    println!(
        "Scanout readback max us: {:.3}",
        report.scanout_readback_max_nanoseconds as f64 / 1_000.0
    );
    println!(
        "Scanout readback transfer avg us: {:.3}",
        report.scanout_readback_phase_average_us(report.scanout_readback_transfer_nanoseconds)
    );
    println!(
        "Scanout readback composite avg us: {:.3}",
        report.scanout_readback_phase_average_us(report.scanout_readback_composite_nanoseconds)
    );
    println!(
        "Scanout readbacks deferred-serviced: {}",
        report.scanout_readbacks_deferred
    );
    println!("Scanout IOSurface blits: {}", report.scanout_blits);
    println!(
        "Scanout IOSurface blit avg us: {:.3}",
        if report.scanout_blits == 0 {
            0.0
        } else {
            report.scanout_blit_nanoseconds as f64 / report.scanout_blits as f64 / 1_000.0
        }
    );
    println!(
        "Scanout IOSurface verify: {} matched / {} mismatched",
        report.iosurface_verify_matched, report.iosurface_verify_mismatched
    );
    println!(
        "Scanout readback effective GB/s: {:.3}",
        report.scanout_readback_effective_gbps()
    );
    println!(
        "Scanout throttle ratio: {:.2}%",
        report.scanout_throttle_percent()
    );
    if report.error_responses.is_empty() {
        println!("Error responses: none");
    } else {
        println!("Error responses: {}", report.error_responses.len());
        for response in report.error_responses.iter().take(5) {
            println!("- {response}");
        }
    }
    println!(
        "P3 Windows 3D trace gate: {}",
        if blockers.is_empty() { "PASS" } else { "FAIL" }
    );
    if blockers.is_empty() {
        println!("Blockers: none");
    } else {
        println!("Blockers:");
        for blocker in blockers {
            println!("- {blocker}");
        }
    }
}

//! Deferred GPU notifications must consume both the PBA and their source latch.
#[path = "virtio_gpu_msix/platform.rs"]
mod platform;
#[path = "virtio_gpu_msix/support.rs"]
mod support;
use support::*;

#[test]
fn vector_masked_resize_replays_once_and_rearms() {
    let mut gpu = Gpu::new(0);
    gpu.mask(0, true);
    assert!(gpu.dev.request_display_resolution(1600, 900));
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(gpu.pba(), 1);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    gpu.mask(0, false);
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert_eq!(gpu.pba(), 0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert!(!gpu.dev.request_display_resolution(1600, 900));
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert!(gpu.dev.request_display_resolution(1920, 1080));
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn function_masked_resize_replays_once() {
    let mut gpu = Gpu::new(0);
    assert!(gpu.dev.request_display_resolution(1600, 900));
    assert!(gpu.dev.drain_pending_msix(true, true).is_empty());
    assert_eq!(gpu.pba(), 1);
    assert!(gpu.dev.drain_pending_msix(true, true).is_empty());
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert_eq!(gpu.pba(), 0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn queue_only_deferred_completion_is_delivered_once() {
    let mut gpu = Gpu::new(1);
    gpu.mask(0, true);
    gpu.complete_display_info(0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(gpu.pba(), 1);
    gpu.mask(0, false);
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert_eq!(gpu.pba(), 0);
    assert!(!gpu.dev.stats().queues[0].pending_msix);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn shared_vector_consumes_queue_and_config_notifications() {
    let mut gpu = Gpu::new(0);
    gpu.mask(0, true);
    gpu.complete_display_info(0);
    assert!(gpu.dev.request_display_resolution(1600, 900));
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(gpu.pba(), 1);
    gpu.mask(0, false);
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert!(!gpu.dev.stats().queues[0].pending_msix);
    assert_eq!(gpu.pba(), 0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn unrelated_queue_delivery_preserves_masked_config_notification() {
    let mut gpu = Gpu::new(1);
    gpu.mask(0, true);
    gpu.mask(1, true);
    gpu.complete_display_info(0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(gpu.pba(), 1);
    assert!(gpu.dev.request_display_resolution(1600, 900));
    gpu.mask(0, false);
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert_eq!(gpu.pba(), 2);
    assert!(!gpu.dev.stats().queues[0].pending_msix);
    gpu.mask(1, false);
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(1)]);
    assert_eq!(gpu.pba(), 0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn runtime_reset_discards_masked_resize_and_allows_new_event() {
    let mut gpu = Gpu::new(0);
    gpu.mask(0, true);
    assert!(gpu.dev.request_display_resolution(1600, 900));
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(gpu.pba(), 1);
    gpu.dev.reset_runtime_state();
    assert_eq!(gpu.pba(), 0);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    gpu.program(0);
    assert!(gpu.dev.request_display_resolution(1920, 1080));
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn existing_output_does_not_consume_a_new_config_event() {
    let mut gpu = Gpu::new(0);
    let mut out = vec![message(0)];
    assert!(gpu.dev.request_display_resolution(1600, 900));
    gpu.dev.drain_pending_msix_into(true, false, &mut out);
    assert_eq!(out, vec![message(0), message(0)]);
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
}

#[test]
fn config_event_and_isr_acknowledgements_are_independent_of_msix_delivery() {
    let mut gpu = Gpu::new(0);
    gpu.mask(0, true);
    assert!(gpu.dev.request_display_resolution(1600, 900));
    assert!(gpu.dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(gpu.read(0x2000, 4), 1); // DISPLAY remains until events_clear
    assert_eq!(gpu.read(0x1000, 1), 2); // PCI ISR read-to-clear
    assert_eq!(gpu.pba(), 1);
    gpu.mask(0, false);
    assert_eq!(gpu.dev.drain_pending_msix(true, false), vec![message(0)]);
    assert_eq!(gpu.read(0x2000, 4), 1);
    assert_eq!(gpu.read(0x1000, 1), 0);
    gpu.write(0x2004, 4, 1);
    assert_eq!(gpu.read(0x2000, 4), 0);
    assert_eq!(gpu.pba(), 0);
}

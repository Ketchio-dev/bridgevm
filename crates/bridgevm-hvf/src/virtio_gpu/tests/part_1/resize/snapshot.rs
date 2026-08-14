//! Active/requested geometry snapshot compatibility.

use super::super::*;
use super::helpers::*;

#[test]
fn snapshot_v2_round_trips_active_and_pending_geometry() {
    let mut source = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut source, 1, 4, 3);
    source.gpu.scanout_resource = Some(1);
    source.gpu.scanout.fill(0x3c);
    assert!(source.request_display_resolution(6, 5));
    let state = source.snapshot_state();

    let mut restored = VirtioPciGpu::new(4, 3);
    assert!(restored.request_display_resolution(6, 5));
    restored.restore_state(&state);

    assert_eq!(restored.display_resolution(), (6, 5));
    let active = restored.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (4, 3, 16));
    assert!(active.bytes.iter().all(|byte| *byte == 0x3c));
}

#[test]
fn snapshot_v1_defaults_requested_geometry_to_active_geometry() {
    let mut source = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut source, 1, 4, 3);
    source.gpu.scanout_resource = Some(1);
    source.gpu.scanout.fill(0x69);
    let v2 = source.snapshot_state();

    let mut v1 = Vec::with_capacity(v2.len() - 8);
    v1.extend_from_slice(&1u32.to_le_bytes());
    v1.extend_from_slice(&v2[4..12]);
    v1.extend_from_slice(&v2[20..]);

    let mut restored = VirtioPciGpu::new(4, 3);
    restored.restore_state(&v1);

    assert_eq!(restored.display_resolution(), (4, 3));
    let active = restored.scanout().unwrap();
    assert_eq!((active.width, active.height), (4, 3));
    assert!(active.bytes.iter().all(|byte| *byte == 0x69));
}

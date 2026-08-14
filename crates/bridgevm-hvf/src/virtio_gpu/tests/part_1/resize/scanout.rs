//! 2D scanout resize transitions.

use super::super::*;
use super::helpers::*;

#[test]
fn host_resize_preserves_active_frame_until_guest_switches_modes() {
    let mut dev = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut dev, 1, 4, 3);
    dev.gpu.scanout_resource = Some(1);
    dev.gpu.scanout.fill(0x5a);
    let original = dev.gpu.scanout.clone();

    assert!(dev.request_display_resolution(6, 5));
    assert_eq!(dev.display_resolution(), (6, 5));
    let active = dev.gpu.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (4, 3, 16));
    assert_eq!(active.bytes, original);
    assert_eq!(dev.gpu.scanout_resource, Some(1));

    install_2d_resource(&mut dev, 2, 6, 5);
    set_2d_scanout(&mut dev, 2, 6, 5);

    let active = dev.gpu.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (6, 5, 24));
    assert_eq!(active.bytes, vec![0; 6 * 5 * 4]);
    assert_eq!(dev.gpu.scanout_resource, Some(2));
}

#[test]
fn nonmatching_scanout_does_not_commit_requested_geometry() {
    let mut dev = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut dev, 1, 4, 3);
    dev.gpu.scanout_resource = Some(1);
    dev.gpu.scanout.fill(0xa5);
    let original = dev.gpu.scanout.clone();

    assert!(dev.request_display_resolution(6, 5));
    install_2d_resource(&mut dev, 2, 5, 4);
    set_2d_scanout(&mut dev, 2, 5, 4);

    assert_eq!(dev.display_resolution(), (6, 5));
    let active = dev.gpu.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (4, 3, 16));
    assert_eq!(active.bytes, original);
}

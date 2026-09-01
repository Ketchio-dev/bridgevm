use super::*;
#[path = "result_test_fixture.rs"]
mod fixture;
use fixture::{valid_ram, write_u32, write_u64};

#[test]
fn accepts_complete_post_exit_evidence() {
    let result = validate(&valid_ram()).expect("valid result");
    assert_eq!(result.stage, POST_EXIT);
    assert_eq!(result.arch, REQUIRED_ARCH);
    assert_eq!(result.file_systems, 1);
    assert_eq!(
        result.image_base,
        bridgevm_hvf::machine::bridgevm_pc::RAM_BASE + 0x4000
    );
    assert_eq!(result.image_size, 0x1000);
    assert_eq!(result.memory_map_size, 48);
    assert_eq!(result.descriptor_size, 48);
    assert_eq!(result.descriptor_version, 1);
    assert_eq!(result.exit_attempts, 1);
    assert_eq!(result.gop_handles, 1);
    assert_eq!(result.framebuffer_size, 0x1000);
}

#[test]
fn rejects_a_pre_exit_stage() {
    let mut ram = valid_ram();
    write_u32(&mut ram[OFFSET..], 12, POST_EXIT - 1);
    assert!(validate(&ram).unwrap_err().contains("header is invalid"));
}

#[test]
fn rejects_absent_graphics_output_evidence() {
    let mut ram = valid_ram();
    write_u64(&mut ram[OFFSET..], 120, 0);
    assert!(validate(&ram)
        .unwrap_err()
        .contains("GOP evidence is incomplete"));
}

#[test]
fn rejects_a_framebuffer_outside_guest_ram() {
    let mut ram = valid_ram();
    write_u64(&mut ram[OFFSET..], 128, 0x1000);
    assert!(validate(&ram)
        .unwrap_err()
        .contains("GOP evidence is incomplete"));
    write_u32(&mut ram[OFFSET..], 12, READY_TO_BOOT);
    assert!(validate_windows_start(&ram)
        .unwrap_err()
        .contains("handoff is incomplete"));
}

#[test]
fn accepts_a_windows_image_that_remains_inside_start_image() {
    let mut ram = valid_ram();
    write_u32(&mut ram[OFFSET..], 12, READY_TO_BOOT);
    let result = validate_windows_start(&ram).expect("Windows handoff");
    assert_eq!(result.stage, READY_TO_BOOT);
    assert_eq!(result.arch, REQUIRED_ARCH);
    assert_eq!(result.gop_handles, 1);
}

#[test]
fn rejects_a_windows_image_that_returned_to_bds() {
    let mut ram = valid_ram();
    write_u32(&mut ram[OFFSET..], 12, 0x8000_0008);
    assert!(validate_windows_start(&ram)
        .unwrap_err()
        .contains("handoff is incomplete"));
}

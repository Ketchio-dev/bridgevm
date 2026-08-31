use super::*;

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn valid_ram() -> Vec<u8> {
    let mut ram = vec![0; 0x10000];
    let result = &mut ram[OFFSET..OFFSET + 128];
    write_u64(result, 0, MAGIC);
    write_u32(result, 8, 1);
    write_u32(result, 12, POST_EXIT);
    write_u64(result, 24, REQUIRED_ARCH);
    write_u64(result, 32, 1);
    write_u64(result, 40, board::RAM_BASE + 0x1000);
    write_u64(result, 48, board::RAM_BASE + 0x2000);
    write_u64(result, 56, board::RAM_BASE + 0x4000);
    write_u64(result, 64, 0x1000);
    write_u64(result, 72, 48);
    write_u64(result, 80, 7);
    write_u64(result, 88, 48);
    write_u32(result, 96, 1);
    write_u32(result, 100, 1);
    write_u64(result, 104, board::RAM_BASE + 0x6000);
    write_u64(result, 112, board::RAM_BASE + 0x7000);
    ram
}

#[test]
fn accepts_complete_post_exit_evidence() {
    let result = validate(&valid_ram()).expect("valid result");
    assert_eq!(result.stage, POST_EXIT);
    assert_eq!(result.arch, REQUIRED_ARCH);
    assert_eq!(result.file_systems, 1);
    assert_eq!(result.image_base, board::RAM_BASE + 0x4000);
    assert_eq!(result.image_size, 0x1000);
    assert_eq!(result.memory_map_size, 48);
    assert_eq!(result.descriptor_size, 48);
    assert_eq!(result.descriptor_version, 1);
    assert_eq!(result.exit_attempts, 1);
}

#[test]
fn rejects_a_pre_exit_stage() {
    let mut ram = valid_ram();
    write_u32(&mut ram[OFFSET..], 12, POST_EXIT - 1);
    assert!(validate(&ram).unwrap_err().contains("header is invalid"));
}

#[test]
fn accepts_a_windows_image_that_remains_inside_start_image() {
    let mut ram = valid_ram();
    write_u32(&mut ram[OFFSET..], 12, READY_TO_BOOT);
    let result = validate_windows_start(&ram).expect("Windows handoff");
    assert_eq!(result.stage, READY_TO_BOOT);
    assert_eq!(result.arch, REQUIRED_ARCH);
}

#[test]
fn rejects_a_windows_image_that_returned_to_bds() {
    let mut ram = valid_ram();
    write_u32(&mut ram[OFFSET..], 12, 0x8000_0008);
    assert!(validate_windows_start(&ram)
        .unwrap_err()
        .contains("handoff is incomplete"));
}

//! Byte-level fixture for a complete version-2 firmware boot result.

use super::super::{MAGIC, OFFSET, POST_EXIT, REQUIRED_ARCH};
use bridgevm_hvf::machine::bridgevm_pc as board;

pub(super) fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn valid_ram() -> Vec<u8> {
    let mut ram = vec![0; 0x10000];
    let result = &mut ram[OFFSET..OFFSET + 144];
    write_u64(result, 0, MAGIC);
    write_u32(result, 8, 2);
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
    write_u64(result, 120, 1);
    write_u64(result, 128, board::RAM_BASE + 0x8000);
    write_u64(result, 136, 0x1000);
    ram
}

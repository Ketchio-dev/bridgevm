//! RAM-range checks shared by the BDS and Windows-start result validators.

use super::wire::BootResult;
use bridgevm_hvf::machine::bridgevm_pc as board;

pub(super) fn valid_loaded_image(result: &BootResult, ram_len: usize) -> bool {
    let ram_end = board::RAM_BASE + ram_len as u64;
    [result.image_base, result.system_table, result.boot_services]
        .into_iter()
        .all(|value| (board::RAM_BASE..ram_end).contains(&value))
        && result.image_size != 0
        && result.image_base + result.image_size <= ram_end
}

pub(super) fn valid_graphics(result: &BootResult, ram_len: usize) -> bool {
    let ram_end = board::RAM_BASE + ram_len as u64;
    result.gop_handles != 0
        && result.framebuffer_size != 0
        && (board::RAM_BASE..ram_end).contains(&result.framebuffer_base)
        && result.framebuffer_base + result.framebuffer_size <= ram_end
}

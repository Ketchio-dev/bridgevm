//! Full-frame readback promotion policy.

use super::Rect;

pub(crate) fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn full_frame_readback(
    resource_width: u32,
    resource_height: u32,
    scanout_width: u32,
    scanout_height: u32,
    rect: Rect,
) -> bool {
    resource_width == scanout_width
        && resource_height == scanout_height
        && rect.x == 0
        && rect.y == 0
        && rect.width >= scanout_width
        && rect.height >= scanout_height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_full_frame_can_promote_by_swapping_buffers() {
        assert!(full_frame_readback(
            1600,
            900,
            1600,
            900,
            Rect {
                x: 0,
                y: 0,
                width: 1600,
                height: 900,
            },
        ));
    }

    #[test]
    fn partial_or_differently_strided_frames_still_composite() {
        let partial = Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 900,
        };
        assert!(!full_frame_readback(1600, 900, 1600, 900, partial));
        assert!(!full_frame_readback(1280, 900, 1600, 900, partial));
    }
}

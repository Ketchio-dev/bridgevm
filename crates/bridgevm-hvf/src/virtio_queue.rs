//! Shared bounds helpers for virtio queue registers.

pub(crate) fn clamp_u16(value: u64, max: u16) -> u16 {
    value.min(u64::from(max)) as u16
}

#[cfg(test)]
mod tests {
    use super::clamp_u16;

    #[test]
    fn queue_size_clamps_before_narrowing() {
        let max = 256;
        for (value, expected) in [
            (u64::from(max), max),
            (u64::from(max) + 1, max),
            (65_535, max),
            (65_536, max),
            (u64::MAX, max),
        ] {
            assert_eq!(clamp_u16(value, max), expected);
        }
    }
}

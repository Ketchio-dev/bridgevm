use super::*;

#[test]
fn rnd32_preserves_contiguous_entropy_at_every_request_length() {
    for bits in 1u32..=96 {
        let ret = call(func::RND32, u64::from(bits));
        let mut bytes = [0u8; 12];
        for (index, byte) in bytes.iter_mut().take(bits.div_ceil(8) as usize).enumerate() {
            *byte = index as u8 + 1;
        }
        let expected = u128::from_le_bytes({
            let mut wide = [0u8; 16];
            wide[..12].copy_from_slice(&bytes);
            wide
        }) & ((1u128 << bits) - 1);
        let actual = u128::from(ret.x3) | (u128::from(ret.x2) << 32) | (u128::from(ret.x1) << 64);
        assert_eq!(ret.x0, status::SUCCESS);
        assert_eq!(
            actual, expected,
            "{bits}-bit request must preserve every entropy byte"
        );
        assert_eq!(ret.x1 >> 32 | ret.x2 >> 32 | ret.x3 >> 32, 0);
    }
}

#[test]
fn rnd32_provider_failure_returns_no_register_data() {
    let mut provider = FailingSource::new(EntropyError::Unavailable);
    let ret = handle_call(func::RND32, 96, &mut provider).expect("known function");
    assert_eq!(ret.x0, status::NO_ENTROPY);
    assert_eq!((ret.x1, ret.x2, ret.x3), (0, 0, 0));
    assert_eq!(provider.calls, 1);
}

//! Protocol tests for the SMCCC TRNG service.
//!
//! These prove the request/response rules and, most importantly, that no
//! failure path can hand the guest a predictable value.

use super::*;

/// Counts up from a seed. Deterministic on purpose: it makes register layout
/// and masking assertions exact. It is a test double, never a provider.
struct CountingSource {
    next: u8,
    calls: usize,
}

impl CountingSource {
    fn new(start: u8) -> Self {
        Self {
            next: start,
            calls: 0,
        }
    }
}

impl EntropySource for CountingSource {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        self.calls += 1;
        for byte in out.iter_mut() {
            *byte = self.next;
            self.next = self.next.wrapping_add(1);
        }
        Ok(())
    }
}

struct FailingSource {
    error: EntropyError,
    calls: usize,
}

impl FailingSource {
    fn new(error: EntropyError) -> Self {
        Self { error, calls: 0 }
    }
}

impl EntropySource for FailingSource {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        self.calls += 1;
        // A hostile-but-plausible provider: it writes before reporting failure.
        // The protocol must still return nothing.
        for byte in out.iter_mut() {
            *byte = 0xAB;
        }
        Err(self.error)
    }
}

fn call(function_id: u64, x1: u64) -> SmcccReturn {
    handle_call(function_id, x1, &mut CountingSource::new(1)).expect("known function")
}

#[test]
fn version_reports_trng_1_0() {
    assert_eq!(call(func::VERSION, 0).x0, 0x1_0000);
}

#[test]
fn features_reports_only_implemented_functions() {
    for id in [
        func::VERSION,
        func::FEATURES,
        func::GET_UUID,
        func::RND32,
        func::RND64,
    ] {
        assert_eq!(
            call(func::FEATURES, id).x0,
            status::SUCCESS,
            "implemented function {id:#x} must be advertised"
        );
    }

    // The previous implementation answered SUCCESS unconditionally, which told
    // the guest that unimplemented calls existed.
    for id in [0x8400_0054u64, 0x8400_0000, 0xc400_0054, 0] {
        assert_eq!(
            call(func::FEATURES, id).x0,
            status::NOT_SUPPORTED,
            "unimplemented function {id:#x} must not be advertised"
        );
    }
}

#[test]
fn get_uuid_returns_the_specified_service_uuid() {
    let ret = call(func::GET_UUID, 0);
    assert_eq!([ret.x0, ret.x1, ret.x2, ret.x3], UUID_WORDS);
}

#[test]
fn unknown_function_is_not_handled_here() {
    assert!(handle_call(0x8400_0009, 0, &mut CountingSource::new(1)).is_none());
}

#[test]
fn rnd64_fills_x3_first() {
    // Bytes 1..=8 are the first word and must land in X3.
    let ret = call(func::RND64, 64);
    assert_eq!(ret.x0, status::SUCCESS);
    assert_eq!(ret.x3, u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8]));
    assert_eq!(ret.x2, 0, "unrequested registers must be zero");
    assert_eq!(ret.x1, 0, "unrequested registers must be zero");
}

#[test]
fn rnd64_fills_all_three_registers_at_the_maximum() {
    let ret = call(func::RND64, 192);
    assert_eq!(ret.x0, status::SUCCESS);
    assert_eq!(ret.x3, u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8]));
    assert_eq!(ret.x2, u64::from_le_bytes([9, 10, 11, 12, 13, 14, 15, 16]));
    assert_eq!(ret.x1, u64::from_le_bytes([17, 18, 19, 20, 21, 22, 23, 24]));
}

#[test]
fn rnd32_packs_into_the_low_half_of_each_register() {
    let ret = call(func::RND32, 96);
    assert_eq!(ret.x0, status::SUCCESS);
    for (name, value) in [("x1", ret.x1), ("x2", ret.x2), ("x3", ret.x3)] {
        assert_eq!(
            value >> 32,
            0,
            "{name} must not carry bits above the 32-bit request"
        );
    }
    assert_eq!(ret.x3, u32::from_le_bytes([1, 2, 3, 4]) as u64);
}

#[test]
fn requests_are_masked_to_the_requested_bit_count() {
    for bits in [
        1u32, 7, 8, 31, 32, 33, 63, 64, 65, 95, 96, 127, 128, 191, 192,
    ] {
        let ret = call(func::RND64, u64::from(bits));
        assert_eq!(ret.x0, status::SUCCESS, "{bits} bits must succeed");

        let total = (ret.x3 as u128) | ((ret.x2 as u128) << 64);
        let high = ret.x1;
        let returned_bits = if bits <= 128 {
            assert_eq!(high, 0, "{bits} bits must leave X1 clear");
            128 - total.leading_zeros()
        } else {
            192 - (high.leading_zeros() + if high == 0 { 128 } else { 0 })
        };
        assert!(
            returned_bits <= bits,
            "{bits} bits requested but {returned_bits} significant bits returned"
        );
    }
}

#[test]
fn zero_bits_succeeds_without_entropy() {
    let mut source = CountingSource::new(1);
    let ret = handle_call(func::RND64, 0, &mut source).expect("known function");
    assert_eq!(ret.x0, status::SUCCESS);
    assert_eq!((ret.x1, ret.x2, ret.x3), (0, 0, 0));
    assert_eq!(source.calls, 0, "a zero-bit request must not draw entropy");
}

#[test]
fn oversized_requests_are_invalid() {
    assert_eq!(
        call(func::RND64, 193).x0,
        status::INVALID_PARAMETER,
        "RND64 accepts at most 192 bits"
    );
    assert_eq!(
        call(func::RND32, 97).x0,
        status::INVALID_PARAMETER,
        "RND32 accepts at most 96 bits"
    );
    assert_eq!(call(func::RND64, u64::MAX).x0, status::INVALID_PARAMETER);
}

#[test]
fn provider_failure_returns_no_entropy_and_no_data() {
    let mut source = FailingSource::new(EntropyError::Unavailable);
    let ret = handle_call(func::RND64, 192, &mut source).expect("known function");

    assert_eq!(ret.x0, status::NO_ENTROPY);
    assert_eq!(
        (ret.x1, ret.x2, ret.x3),
        (0, 0, 0),
        "a failed call must not leak provider buffer contents"
    );
    assert_eq!(source.calls, 1);
}

#[test]
fn unsupported_provider_returns_not_supported() {
    let mut source = FailingSource::new(EntropyError::Unsupported);
    let ret = handle_call(func::RND32, 32, &mut source).expect("known function");
    assert_eq!(ret.x0, status::NOT_SUPPORTED);
    assert_eq!((ret.x1, ret.x2, ret.x3), (0, 0, 0));
}

#[test]
fn status_codes_match_the_specification() {
    // Written out rather than derived, so a sign or width mistake is visible.
    assert_eq!(status::SUCCESS, 0);
    assert_eq!(status::NOT_SUPPORTED, 0xffff_ffff_ffff_ffff);
    assert_eq!(status::INVALID_PARAMETER, 0xffff_ffff_ffff_fffe);
    assert_eq!(status::NO_ENTROPY, 0xffff_ffff_ffff_fffd);
}

#[test]
fn output_is_not_a_function_of_a_call_counter() {
    // The defect this module replaces derived entropy from the vCPU exit
    // count, so identical guest requests produced a predictable stream. Two
    // sources at different positions must not collide.
    let mut first = CountingSource::new(1);
    let mut second = CountingSource::new(200);
    let a = handle_call(func::RND64, 192, &mut first).expect("known function");
    let b = handle_call(func::RND64, 192, &mut second).expect("known function");
    assert_ne!((a.x1, a.x2, a.x3), (b.x1, b.x2, b.x3));
}

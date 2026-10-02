use super::*;
struct Source;
impl EntropySource for Source {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        out.fill(0xa5);
        Ok(())
    }
}
#[test]
fn rnd32_ignores_upper_argument_bits() {
    let ret = handle_call(func::RND32, (1u64 << 32) | 32, &mut Source).unwrap();
    assert_eq!(ret.x0, status::SUCCESS);
    assert_eq!(ret.x3, 0xa5a5_a5a5);
}
#[test]
fn features_ignores_upper_query_bits() {
    assert_eq!(
        handle_call(func::FEATURES, (1u64 << 32) | func::RND64, &mut Source)
            .unwrap()
            .x0,
        status::SUCCESS
    );
}
#[test]
fn function_id_uses_w0_for_both_call_conventions() {
    for function in [
        func::VERSION,
        func::FEATURES,
        func::GET_UUID,
        func::RND32,
        func::RND64,
    ] {
        assert_eq!(
            handle_call((1u64 << 32) | function, 0, &mut Source),
            handle_call(function, 0, &mut Source)
        );
    }
}
#[test]
fn rnd64_retains_full_width_bit_count() {
    assert_eq!(
        handle_call(func::RND64, (1u64 << 32) | 32, &mut Source)
            .unwrap()
            .x0,
        status::INVALID_PARAMETER
    );
}

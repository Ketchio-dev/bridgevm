//! Guest network frames reach the NAT parsers before any validation.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    bridgevm_fuzz::nat_packet(data);
});

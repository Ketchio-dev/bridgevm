//! NVMe submission-queue entries come straight from guest RAM, so every one
//! of the 64 bytes is attacker-controlled.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    bridgevm_fuzz::nvme_prp(data);
});

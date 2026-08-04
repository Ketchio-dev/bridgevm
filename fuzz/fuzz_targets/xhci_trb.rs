//! fw_cfg DMA descriptors are guest-written and drive host-side transfers.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    bridgevm_fuzz::xhci_trb(data);
});

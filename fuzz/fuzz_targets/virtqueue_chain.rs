//! virtio-gpu 3D control headers are guest-written.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    bridgevm_fuzz::virtqueue_chain(data);
});

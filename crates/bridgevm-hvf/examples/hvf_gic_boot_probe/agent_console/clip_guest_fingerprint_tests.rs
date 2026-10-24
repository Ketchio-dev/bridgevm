//! The refused-reply fingerprint reads a bounded part of the payload: an
//! oversized text left on the guest clipboard is re-polled every interval, and
//! hashing all of it (up to the 32 MiB line limit) each time stalls the VMM's
//! service path. Count-once still holds for ordinary changes of the text.

use super::*;
use std::hash::Hasher;

const NINE_MIB: usize = 9 * 1024 * 1024;

/// Counts the bytes it is given and hashes nothing.
#[derive(Default)]
struct CountingHasher(usize);

impl Hasher for CountingHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0 += bytes.len();
    }
    fn finish(&self) -> u64 {
        0
    }
}

#[test]
fn a_refused_9_mib_payload_hashes_at_most_about_8_kib_of_it() {
    let payload = base64_encode(&vec![0x5a; NINE_MIB]);
    assert!(
        decode_guest_clip(&payload).is_err(),
        "9 MiB must be refused"
    );
    let mut counted = CountingHasher::default();
    guest_clip_fingerprint(&payload, &mut counted);
    // 4 KiB from each end plus a few length words.
    assert!(counted.0 <= 2 * 4096 + 64, "{} bytes hashed", counted.0);
}

#[test]
fn a_9_mib_text_counts_once_and_a_new_tail_or_length_counts_again() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "n".repeat(NINE_MIB);
    rig.settle();
    rig.settle();
    assert_eq!(rig.h.last_synced.refusals.count, 1);
    // Three bytes longer: its base64 has the same first and last 4 KiB.
    rig.guest = "n".repeat(NINE_MIB + 3);
    rig.settle();
    rig.settle();
    assert_eq!(rig.h.last_synced.refusals.count, 2);
    // Same length and head as the text before, different last byte.
    rig.guest = format!("{}m", "n".repeat(NINE_MIB + 2));
    rig.settle();
    assert_eq!(rig.h.last_synced.refusals.count, 3);
    assert!(rig.mac_sets().is_empty());
}

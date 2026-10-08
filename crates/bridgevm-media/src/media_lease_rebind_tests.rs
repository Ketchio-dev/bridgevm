use super::*;
use crate::media_lease::tests::Scratch;

#[test]
fn successful_rebinding_keeps_a_bounded_set_of_descriptors() {
    let s = Scratch::new("lease-replace-bounded");
    let logical = s.write("logical", b"logical");
    let current = s.write("current", b"initial");
    let staging = s.path("staging");
    let mut lease = MediaLease::acquire([logical.as_path(), current.as_path()]).unwrap();
    let count = lease.files.len();
    for generation in 0..16 {
        fs::write(&staging, generation.to_string()).unwrap();
        lease.extend([staging.as_path()]).unwrap();
        fs::rename(&staging, &current).unwrap();
        lease
            .replace([logical.as_path(), current.as_path()])
            .unwrap();
        assert_eq!(lease.files.len(), count);
        for path in [&logical, &current] {
            assert!(MediaLease::acquire([path.as_path()]).is_err());
        }
    }
}

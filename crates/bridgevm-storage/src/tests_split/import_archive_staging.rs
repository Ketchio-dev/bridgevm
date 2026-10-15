//! Extraction success and failure clean only the exclusively reserved parent.
use super::*;
use crate::extract_bundle_tar;
use std::path::Path;

fn archive(path: &Path, invalid: bool) {
    let mut builder = tar::Builder::new(fs::File::create(path).unwrap());
    let mut header = tar::Header::new_gnu();
    header.set_size(5);
    header.set_mode(0o600);
    header.set_cksum();
    builder
        .append_data(&mut header, "payload", &b"owned"[..])
        .unwrap();
    if invalid {
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_link_name("payload").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "rejected-link", &[][..])
            .unwrap();
    }
    builder.finish().unwrap();
}

#[test]
fn extraction_cleanup_preserves_an_occupied_candidate_on_success_and_error() {
    let fixture = create("bridgevm-import-staging-test").unwrap();
    let peer = fixture.path.join("occupied");
    fs::create_dir(&peer).unwrap();
    fs::write(peer.join("sentinel"), b"peer bytes").unwrap();
    let identity = fs::metadata(&peer).unwrap();
    for invalid in [false, true] {
        let input = fixture.path.join(format!("input-{invalid}.tar"));
        archive(&input, invalid);
        let input_bytes = fs::read(&input).unwrap();
        let owned = fixture.path.join(format!("owned-{invalid}"));
        let guard = reserve([peer.clone(), owned.clone()]).unwrap();
        let bundle = guard.path.join("bundle");
        let result = extract_bundle_tar(&input, &bundle);
        if invalid {
            assert!(matches!(
                result,
                Err(crate::StorageError::UnsupportedBundleEntry(_))
            ));
        } else {
            result.unwrap();
        }
        assert_eq!(fs::read(bundle.join("payload")).unwrap(), b"owned");
        drop(guard);
        assert!(!owned.exists());
        let current = fs::metadata(&peer).unwrap();
        assert_eq!(
            (current.dev(), current.ino()),
            (identity.dev(), identity.ino())
        );
        assert_eq!(fs::read(peer.join("sentinel")).unwrap(), b"peer bytes");
        assert_eq!(fs::read(input).unwrap(), input_bytes);
    }
}

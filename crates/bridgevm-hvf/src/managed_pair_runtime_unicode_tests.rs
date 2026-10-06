use super::super::tests::fixture;
use super::super::*;

#[cfg(target_os = "macos")]
#[test]
fn absent_unicode_alias_outputs_are_refused() {
    for (left, right) in [("output-é", "output-e\u{301}"), ("output-É", "output-é")] {
        let (scratch, mut media) = fixture("unicode-alias-output");
        let probe = scratch.write(left, b"probe");
        if !scratch.path(right).exists() {
            fs::remove_file(probe).unwrap();
            continue;
        }
        fs::remove_file(probe).unwrap();
        media.flash_vars.snapshot_path = Some(scratch.path(left));
        media.nvme_disk.as_mut().unwrap().snapshot_path = Some(scratch.path(right));
        assert!(
            super::super::acquire(&mut media).is_err(),
            "{left:?} aliases {right:?}"
        );
        assert!(!scratch.path(left).exists());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn unicode_comparison_preserves_case_sensitive_distinctions() {
    use std::ffi::OsStr;
    let equal = |left, right, insensitive| {
        super::unicode::equivalent(OsStr::new(left), OsStr::new(right), insensitive).unwrap()
    };
    assert!(equal("é", "e\u{301}", false));
    assert!(!equal("É", "é", false));
    assert!(equal("É", "é", true));
    assert!(!equal("é", "e", true));
}

#[test]
fn distinct_outputs_below_absent_parent_remain_supported() {
    let (scratch, mut media) = fixture("different-absent-output");
    media.flash_vars.snapshot_path = Some(scratch.path("missing/vars"));
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(scratch.path("missing/disk"));
    assert!(super::super::acquire(&mut media).is_ok());
    assert!(!scratch.path("missing").exists());
}

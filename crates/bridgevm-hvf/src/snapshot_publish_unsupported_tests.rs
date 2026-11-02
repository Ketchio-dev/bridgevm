//! The exchange refusal names the volume type the scratch volume reports.

use super::*;

#[test]
fn enotsup_names_the_operation_and_the_volume_type_and_other_errors_pass_through() {
    let scratch = std::env::temp_dir();
    let volume = volume_type(&scratch).expect("the scratch volume reports its type");
    assert!(!volume.is_empty() && volume.is_ascii(), "{volume:?}");
    let refused = refuse(io::Error::from_raw_os_error(libc::ENOTSUP), &scratch);
    assert_eq!(refused.kind(), io::ErrorKind::Unsupported);
    let text = refused.to_string();
    let prefix = format!(
        "{} is on a volume of type {volume} that ",
        scratch.display()
    );
    assert!(text.starts_with(&prefix), "{text}");
    assert!(text.ends_with("; it was left unchanged"), "{text}");
    for errno in [libc::EXDEV, libc::EIO, libc::ENOENT] {
        let passed = refuse(io::Error::from_raw_os_error(errno), &scratch);
        assert_eq!(passed.raw_os_error(), Some(errno));
    }
    assert_eq!(
        volume_type(Path::new("/nonexistent-bridgevm-volume-probe")),
        None
    );
}

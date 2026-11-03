//! The exchange refusal names the volume type the scratch volume reports
//! (macOS); elsewhere it says only "a volume".

use super::*;

#[test]
fn enotsup_names_the_operation_and_the_volume_type_and_other_errors_pass_through() {
    let scratch = std::env::temp_dir();
    let volume = if cfg!(target_os = "macos") {
        let name = volume_type(&scratch).expect("the scratch volume reports its type");
        assert!(!name.is_empty() && name.is_ascii(), "{name:?}");
        format!("a volume of type {name}")
    } else {
        assert_eq!(volume_type(&scratch), None);
        "a volume".to_string()
    };
    let refused = refuse(io::Error::from_raw_os_error(libc::ENOTSUP), &scratch);
    assert_eq!(refused.kind(), io::ErrorKind::Unsupported);
    let text = refused.to_string();
    let prefix = format!("{} is on {volume} that ", scratch.display());
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

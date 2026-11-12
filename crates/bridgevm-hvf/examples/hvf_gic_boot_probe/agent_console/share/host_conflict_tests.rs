use super::*;

#[test]
fn unsupported_links_preserve_conflicts_without_clobbering_or_retrying() {
    let f = Fixture::new();
    let root = f.root();
    std::fs::write(root.join("report.docx"), b"newer Mac save").unwrap();
    std::fs::write(root.join("report (Windows copy).docx"), b"previous conflict").unwrap();
    LINK_FAULT.with(|fault| fault.set(Some(libc::ENOTSUP))); // real exFAT returned errno 45
    let result = Destination::open(&root, "report.docx", true).unwrap()
        .write_at_unless_changed(b"Windows bytes", None, Some(Some((3, 1))));
    LINK_FAULT.with(|fault| fault.set(None));
    assert!(result.as_ref().is_err_and(is_mac_changed), "expected MacChanged, got {result:?}");
    assert_eq!(std::fs::read(root.join("report.docx")).unwrap(), b"newer Mac save");
    assert_eq!(std::fs::read(root.join("report (Windows copy).docx")).unwrap(), b"previous conflict");
    assert_eq!(std::fs::read(root.join("report (Windows copy 2).docx")).unwrap(), b"Windows bytes");
}

#[test]
fn failed_conflict_copy_removes_partial_and_settles_mac_change() {
    let f = Fixture::new();
    std::fs::write(f.root().join("report.docx"), b"newer Mac save").unwrap();
    LINK_FAULT.with(|fault| fault.set(Some(libc::ENOTSUP)));
    CONFLICT_COPY_FAULT.with(|fault| fault.set(Some(libc::ENOSPC)));
    let result = Destination::open(&f.root(), "report.docx", true).unwrap()
        .write_at_unless_changed(b"Windows bytes", None, Some(Some((3, 1))));
    LINK_FAULT.with(|fault| fault.set(None));
    CONFLICT_COPY_FAULT.with(|fault| fault.set(None));
    assert!(result.as_ref().is_err_and(is_mac_changed));
    assert_eq!(std::fs::read(f.root().join("report.docx")).unwrap(), b"newer Mac save");
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 1, "partial conflict or staging file leaked");
}

#[test]
#[ignore = "scratch real-volume script supplies BRIDGEVM_TEST_EXFAT_ROOT"]
fn real_exfat_conflict_publication() {
    let root = std::path::PathBuf::from(std::env::var_os("BRIDGEVM_TEST_EXFAT_ROOT").unwrap());
    std::fs::write(root.join("report.docx"), b"newer Mac save").unwrap();
    std::fs::write(root.join("report (Windows copy).docx"), b"previous conflict").unwrap();
    let result = Destination::open(&root, "report.docx", true).unwrap()
        .write_at_unless_changed(b"Windows bytes", None, Some(Some((3, 1))));
    println!("REAL_EXFAT_PUBLICATION {result:?}");
    assert!(result.as_ref().is_err_and(is_mac_changed), "expected settled MacChanged, got {result:?}");
    assert_eq!(std::fs::read(root.join("report.docx")).unwrap(), b"newer Mac save");
    assert_eq!(std::fs::read(root.join("report (Windows copy).docx")).unwrap(), b"previous conflict");
    assert_eq!(std::fs::read(root.join("report (Windows copy 2).docx")).unwrap(), b"Windows bytes");
}

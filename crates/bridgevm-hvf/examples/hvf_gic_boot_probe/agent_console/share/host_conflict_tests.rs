use super::*;

#[test]
fn fetched_mark_is_on_disk_after_stage_sync_and_before_replacement() {
    let f=Fixture::new();let root=f.root();let path=root.join("doc.bin");
    std::fs::write(&path,b"old Mac").unwrap();
    let vars=root.with_extension("vars");
    let (file,_,_,error)=crate::agent_console::state::sync_file::SyncStateFile::open(&vars,&root);
    assert!(error.is_none());
    let mut called=false;
    Destination::open(&root,"doc.bin",true).unwrap().write_at_unless_changed(b"Windows edit",None,None,false,|| {
        assert_eq!(std::fs::read(&path).unwrap(),b"old Mac","callback ran after rename");
        assert!(std::fs::read_dir(&root).unwrap().filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().starts_with(TEMP_PREFIX) && std::fs::read(e.path()).ok().as_deref()==Some(&b"Windows edit"[..])),"callback ran before staging");
        file.mark(["doc.bin"]).unwrap();
        let mark=std::fs::read_to_string(format!("{}.share-sync-puts",vars.display())).unwrap();
        assert!(mark.contains("mark ZG9jLmJpbg=="),"mark absent before rename");
        called=true;
    }).unwrap();
    assert!(called);assert_eq!(std::fs::read(&path).unwrap(),b"Windows edit");
    drop(file);
    for suffix in [".share-sync",".share-sync-puts"] { let _=std::fs::remove_file(format!("{}{suffix}",vars.display())); }
}

#[test]
fn mac_save_during_mark_callback_is_kept_for_memory_and_staged_publication() {
    for staged in [false,true] {
        let f=Fixture::new();let root=f.root();let path=root.join("doc.bin");
        std::fs::write(&path,b"original").unwrap();
        let meta=std::fs::metadata(&path).unwrap();
        let seen=(meta.len(),crate::agent_console::file_mtime_ms_from_meta(&meta).unwrap());
        let destination=Destination::open(&root,"doc.bin",true).unwrap();
        let save=|| std::fs::write(&path,b"new Mac save during mark barrier").unwrap();
        let result=if staged {
            let mut stage=destination.stage().unwrap();stage.file.write_all(b"Windows edit").unwrap();
            destination.publish(stage,None,Some(Some(seen)),false,save)
        } else {
            destination.write_at_unless_changed(b"Windows edit",None,Some(Some(seen)),false,save)
        };
        assert_eq!(std::fs::read(&path).unwrap(),b"new Mac save during mark barrier","Mac save during mark callback was overwritten");
        assert!(result.as_ref().is_err_and(is_mac_changed));
        assert_eq!(std::fs::read(root.join("doc (Windows copy).bin")).unwrap(),b"Windows edit");
        assert_eq!(std::fs::read_dir(root).unwrap().count(),2);
    }
}

#[test]
fn unsupported_links_preserve_conflicts_without_clobbering_or_retrying() {
    let f = Fixture::new();
    let root = f.root();
    std::fs::write(root.join("report.docx"), b"newer Mac save").unwrap();
    std::fs::write(root.join("report (Windows copy).docx"), b"previous conflict").unwrap();
    LINK_FAULT.with(|fault| fault.set(Some(libc::ENOTSUP))); // real exFAT returned errno 45
    let result = Destination::open(&root, "report.docx", true).unwrap()
        .write_at_unless_changed(b"Windows bytes", None, Some(Some((3, 1))), false, || {});
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
        .write_at_unless_changed(b"Windows bytes", None, Some(Some((3, 1))), false, || {});
    LINK_FAULT.with(|fault| fault.set(None));
    CONFLICT_COPY_FAULT.with(|fault| fault.set(None));
    assert!(result.as_ref().is_err_and(is_mac_changed));
    assert_eq!(std::fs::read(f.root().join("report.docx")).unwrap(), b"newer Mac save");
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 1, "partial conflict or staging file leaked");
}

#[test]
fn recovery_preservation_errors_are_write_failures_not_mac_changed() {
    let f = Fixture::new();
    std::fs::write(f.root().join("report.docx"), b"unchanged Mac").unwrap();
    for errno in [libc::ENOSPC, libc::EIO] {
        LINK_FAULT.with(|fault| fault.set(Some(errno)));
        let result = Destination::open(&f.root(), "report.docx", true).unwrap()
            .write_at_unless_changed(b"Windows edit", None, None, true, || {});
        LINK_FAULT.with(|fault| fault.set(None));
        let error = result.unwrap_err();
        assert_eq!(error.raw_os_error(), Some(errno));
        assert!(!is_mac_changed(&error));
        assert_eq!(std::fs::read(f.root().join("report.docx")).unwrap(), b"unchanged Mac");
        assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 1);
    }
}

#[test]
#[ignore = "scratch real-volume script supplies BRIDGEVM_TEST_EXFAT_ROOT"]
fn real_exfat_conflict_publication() {
    let root = std::path::PathBuf::from(std::env::var_os("BRIDGEVM_TEST_EXFAT_ROOT").unwrap());
    std::fs::write(root.join("report.docx"), b"newer Mac save").unwrap();
    std::fs::write(root.join("report (Windows copy).docx"), b"previous conflict").unwrap();
    let result = Destination::open(&root, "report.docx", true).unwrap()
        .write_at_unless_changed(b"Windows bytes", None, Some(Some((3, 1))), false, || {});
    println!("REAL_EXFAT_PUBLICATION {result:?}");
    assert!(result.as_ref().is_err_and(is_mac_changed), "expected settled MacChanged, got {result:?}");
    assert_eq!(std::fs::read(root.join("report.docx")).unwrap(), b"newer Mac save");
    assert_eq!(std::fs::read(root.join("report (Windows copy).docx")).unwrap(), b"previous conflict");
    assert_eq!(std::fs::read(root.join("report (Windows copy 2).docx")).unwrap(), b"Windows bytes");
}

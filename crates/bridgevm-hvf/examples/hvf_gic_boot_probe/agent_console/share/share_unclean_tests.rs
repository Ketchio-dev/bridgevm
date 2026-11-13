use super::*;

fn state_path(vars: &Path) -> PathBuf { PathBuf::from(format!("{}.share-sync", vars.display())) }

#[test]
fn missing_state_is_unclean_before_any_mutation() {
    let rig=Rig::new("missing-state");let vars=vars(&rig);
    let (file,_,_,error)=sync_file::SyncStateFile::open(&vars,&rig.dir);
    assert!(error.is_none());assert!(file.previous_unclean);
    assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session unclean"));
}

#[test]
fn kill_without_finish_keeps_unclean() {
    let rig=Rig::new("kill-no-finish");let vars=vars(&rig);
    let (file,_,_,_)=sync_file::SyncStateFile::open(&vars,&rig.dir);drop(file);
    let (file,_,_,_)=sync_file::SyncStateFile::open(&vars,&rig.dir);assert!(file.previous_unclean);
}

#[test]
fn old_format_loads_versions_but_is_unclean() {
    let (mut rig,vars)=synced_from_guest("old-state",now_ms()-2*HOUR_MS,true);settle(&mut rig);rig.h.share=None;
    let text=std::fs::read_to_string(state_path(&vars)).unwrap().replace("bridgevm-share-sync 2","bridgevm-share-sync 1");
    let text=text.lines().filter(|l|!l.starts_with("session ")).collect::<Vec<_>>().join("\n")+"\n";
    std::fs::write(state_path(&vars),text).unwrap();
    let (file,files,_,error)=sync_file::SyncStateFile::open(&vars,&rig.dir);
    assert!(error.is_none());assert!(file.previous_unclean);assert!(files.contains_key("doc.bin"));
}

#[test]
fn clean_end_reopens_unclean_before_a_new_boot_can_mutate() {
    let rig=Rig::new("clean-reboot");let vars=vars(&rig);
    let (mut file,_,_,_)=sync_file::SyncStateFile::open(&vars,&rig.dir);
    file.finish_clean(&mut ShareSync::new(512));
    assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session clean"));
    let (file,_,_,_)=sync_file::SyncStateFile::open(&vars,&rig.dir);
    assert!(!file.previous_unclean);
    assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session unclean"));
}

#[test]
fn agent_restart_does_not_clear_unclean() {
    let mut rig=Rig::new("restart-state");let vars=vars(&rig);new_session(&mut rig,Some(&vars));
    rig.h.resync_after_agent_restart("GUEST v3-share2",rig.now);
    assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session unclean"));
}

#[test]
fn share_drop_does_not_clear_unclean() {
    let mut rig=Rig::new("drop-state");let vars=vars(&rig);new_session(&mut rig,Some(&vars));rig.h.share=None;
    assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session unclean"));
}

#[test]
fn clean_finish_joins_latest_versions_and_keeps_other_thread_marks() {
    let rig=Rig::new("finish-marks");let vars=vars(&rig);
    let (mut file,_,_,_)=sync_file::SyncStateFile::open(&vars,&rig.dir);
    let mut engine=ShareSync::new(512);
    engine.on_guest_file("latest".into(),b"latest bytes".to_vec(),Some("g"));
    engine.on_host_write_succeeded("latest",Some(123));
    let marks=file.marks();
    std::thread::spawn(move || sync_file::mark(&marks,["live-put"]).unwrap()).join().unwrap();
    file.finish_clean(&mut engine);
    file.finish(&mut engine); // ShareState's eventual Drop must not undo the joined write.
    let (file,files,marks,_)=sync_file::SyncStateFile::open(&vars,&rig.dir);
    assert!(!file.previous_unclean);assert_eq!(files["latest"].last.size,12);assert!(marks.contains("live-put"));
}

#[test]
fn unclean_stop_without_applied_listing_must_not_launder_recovery() {
    let (mut rig,vars)=synced_from_guest("unclean-no-listing",now_ms()-2*HOUR_MS,true);
    settle(&mut rig);rig.h.share=None;new_session(&mut rig,Some(&vars));
    // Agent never connects: SYSTEM_OFF occurs without running the share service.
    let share=rig.h.share.as_mut().unwrap();
    share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
    rig.h.share=None;
    rig.guest.write("doc.bin",b"damaged Windows after original crash");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
    new_session(&mut rig,Some(&vars));rig.run(24);
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]),"Mac replaced after clean stop without an applied listing");
    assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(&b"damaged Windows after original crash"[..]));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
}

#[test]
fn unclean_rejected_listings_do_not_allow_clean_finish() {
    for failure in ["failed","malformed","too-large"] {
        let (mut rig,vars)=synced_from_guest(&format!("unclean-listing-{failure}"),now_ms()-2*HOUR_MS,true);
        settle(&mut rig);rig.h.share=None;new_session(&mut rig,Some(&vars));
        rig.h.in_flight=Some((ServiceReq::ShareLs,rig.now));
        match failure {
            "failed"=>rig.h.handle_service_reply("ERR LS denied",None,None,rig.now),
            "malformed"=>rig.h.handle_service_reply("LSOK !!!",None,None,rig.now),
            _=>rig.h.share_listing_line_too_long(),
        }
        let share=rig.h.share.as_mut().unwrap();
        share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
        assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session unclean"),"{failure} listing cleared uncertainty");
    }
}

#[test]
fn applied_listing_without_recovery_allows_clean_finish_including_empty_folder() {
    for empty in [false,true] {
        let (mut rig,vars)=synced_from_guest(if empty { "unclean-empty-list" } else { "unclean-same-list" },now_ms()-2*HOUR_MS,true);
        settle(&mut rig);rig.h.share=None;new_session(&mut rig,Some(&vars));
        rig.h.in_flight=Some((ServiceReq::ShareLs,rig.now));
        let listing=if empty { String::new() } else { format!("doc.bin|11|0|{}\n",super::super::dotnet_time(std::fs::metadata(rig.dir.join("doc.bin")).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis())) };
        rig.h.handle_service_reply(&format!("LSOK {}",base64_encode(listing.as_bytes())),None,None,rig.now);
        let share=rig.h.share.as_mut().unwrap();
        assert!(!share.engine.recovery_pending());
        share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
        assert!(std::fs::read_to_string(state_path(&vars)).unwrap().contains("session clean"),"applied empty={empty} listing did not allow clean end");
    }
}

#[test]
fn clean_first_listing_still_accepts_windows_edit() {
    for in_flight in [false,true] {
        let (mut rig, vars) = synced_from_guest(if in_flight { "clean-inflight" } else { "clean-queued" },now_ms()-2*HOUR_MS,true);
        settle(&mut rig);
        rig.guest.write("doc.bin",b"legitimate Windows edit");
        rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
        let req=ServiceReq::ShareGet { name:"doc.bin".into() };
        if in_flight { rig.h.in_flight=Some((req,rig.now)); } else { rig.h.queue.push_back(req); }
        assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]),"not fetched before clean stop");
        new_session(&mut rig,Some(&vars));
        rig.h.in_flight=None;rig.h.queue.clear(); // old wire requests do not survive a boot
        rig.run(24);
        assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"legitimate Windows edit"[..]));
        assert!(rig.host("doc (Windows copy).bin").is_none());
    }
}

#[test]
fn recovery_does_not_divert_a_later_normal_edit_or_loop_on_restart() {
    let (mut rig, vars) = synced_from_guest("recovery-later-edit", now_ms()-2*HOUR_MS, true);
    settle(&mut rig);
    rig.guest.write("doc.bin", b"crash recovery bytes");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms());
    rig.h.share=None;
    new_session(&mut rig,Some(&vars));rig.run(24);settle(&mut rig);
    rig.guest.write("doc.bin",b"later legitimate edit");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms()+HOUR_MS);
    rig.run(24);
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"later legitimate edit"[..]));
    assert!(rig.host("doc (Windows copy 2).bin").is_none());
    settle(&mut rig);rig.h.share=None;new_session(&mut rig,Some(&vars));rig.run(24);
    assert!(rig.host("doc (Windows copy) (Windows copy).bin").is_none());
}

#[test]
fn failed_recovery_copy_single_keeps_windows_until_retry_succeeds() {
    use crate::agent_console::share::host_destination::{LINK_FAULT, CONFLICT_COPY_FAULT};
    let (mut rig, vars) = synced_from_guest("recovery-copy-fails", now_ms()-2*HOUR_MS, true);
    settle(&mut rig);
    rig.guest.write("doc.bin", b"legitimate Windows edit before stop");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
    rig.h.share=None;new_session(&mut rig,Some(&vars));
    let puts=rig.guest.puts.len();
    LINK_FAULT.with(|f| f.set(Some(libc::ENOTSUP)));
    CONFLICT_COPY_FAULT.with(|f| f.set(Some(libc::ENOSPC)));
    rig.run(24);
    LINK_FAULT.with(|f| f.set(None));CONFLICT_COPY_FAULT.with(|f| f.set(None));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"legitimate Windows edit before stop"[..]),"Windows version was overwritten after its copy could not be kept");
    assert_eq!(rig.guest.puts.len(),puts,"Mac version sent after failed preservation");
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));
    assert!(rig.h.share.as_ref().unwrap().engine.recovery_copy("doc.bin"));
    assert!(rig.host("doc (Windows copy).bin").is_none());
    rig.run(24);
    assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(&b"legitimate Windows edit before stop"[..]));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
}

#[test]
fn failed_recovery_copy_single_survives_killed_session() {
    use crate::agent_console::share::host_destination::{LINK_FAULT, CONFLICT_COPY_FAULT};
    let (mut rig, vars) = synced_from_guest("recovery-copy-restart", now_ms()-2*HOUR_MS, true);
    settle(&mut rig);
    rig.guest.write("doc.bin", b"legitimate Windows edit before stop");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
    rig.h.share=None;new_session(&mut rig,Some(&vars));
    let puts=rig.guest.puts.len();
    LINK_FAULT.with(|f| f.set(Some(libc::ENOTSUP)));
    CONFLICT_COPY_FAULT.with(|f| f.set(Some(libc::ENOSPC)));
    let gets=rig.guest.gets.len();
    for _ in 0..16 { rig.run(1); if rig.guest.gets.len()>gets { break; } }
    let paths=[".share-sync",".share-sync-puts"].map(|suffix| PathBuf::from(format!("{}{suffix}",vars.display())));
    let disk=paths.each_ref().map(|p| std::fs::read(p).ok());
    rig.h.share=None; // discard tidy Drop writes: restore exact state at the simulated kill
    for (p,bytes) in paths.iter().zip(disk) { if let Some(bytes)=bytes { std::fs::write(p,bytes).unwrap(); } else { let _=std::fs::remove_file(p); } }
    new_session(&mut rig,Some(&vars));
    LINK_FAULT.with(|f| f.set(None));CONFLICT_COPY_FAULT.with(|f| f.set(None));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"legitimate Windows edit before stop"[..]),"Windows version overwritten after restart");
    assert_eq!(rig.guest.puts.len(),puts,"Mac version sent after failed preservation");
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));

    assert!(rig.host("doc (Windows copy).bin").is_none());
    rig.run(24);
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));
    assert!(rig.host("doc (Windows copy).bin").is_some(),"Windows version overwritten after restart");
    assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(&b"legitimate Windows edit before stop"[..]));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
}

#[test]
fn failed_recovery_copy_single_survives_clean_shutdown() {
    use crate::agent_console::share::host_destination::{LINK_FAULT, CONFLICT_COPY_FAULT};
    let (mut rig, vars) = synced_from_guest("recovery-copy-clean-stop", now_ms()-2*HOUR_MS, true);
    settle(&mut rig);
    rig.guest.write("doc.bin", b"legitimate Windows edit before stop");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
    rig.h.share=None;new_session(&mut rig,Some(&vars));
    let puts=rig.guest.puts.len();
    LINK_FAULT.with(|f| f.set(Some(libc::ENOTSUP)));
    CONFLICT_COPY_FAULT.with(|f| f.set(Some(libc::ENOSPC)));
    let gets=rig.guest.gets.len();
    for _ in 0..16 { rig.run(1); if rig.guest.gets.len()>gets { break; } }
    let share=rig.h.share.as_mut().unwrap();
    share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
    assert!(std::fs::read_to_string(format!("{}.share-sync",vars.display())).unwrap().contains("session unclean"), "unresolved recovery was cleared on clean shutdown");
    new_session(&mut rig,Some(&vars));
    LINK_FAULT.with(|f| f.set(None));CONFLICT_COPY_FAULT.with(|f| f.set(None));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"legitimate Windows edit before stop"[..]),"Windows version overwritten after restart");
    assert_eq!(rig.guest.puts.len(),puts,"Mac version sent after failed preservation");
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));

    assert!(rig.host("doc (Windows copy).bin").is_none());
    rig.run(24);
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));
    assert!(rig.host("doc (Windows copy).bin").is_some(),"Windows version overwritten after restart");
    assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(&b"legitimate Windows edit before stop"[..]));
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
}

#[test]
fn recovery_kept_then_killed_before_put_keeps_both_with_or_without_put_mark() { kept_before_put(false); }

#[test]
fn recovery_kept_then_clean_shutdown_before_put_keeps_both() { kept_before_put(true); }

fn kept_before_put(clean: bool) {
    for marked in [false,true] {
        let (mut rig,vars)=synced_from_guest(if clean { if marked { "kept-clean-mark" } else { "kept-clean-unmarked" } } else if marked { "kept-kill-mark" } else { "kept-kill-unmarked" },now_ms()-2*HOUR_MS,true);
        settle(&mut rig);rig.guest.write("doc.bin",b"Windows before kill");rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
        rig.h.share=None;new_session(&mut rig,Some(&vars));
        let gets=rig.guest.gets.len();
        for _ in 0..16 { rig.run(1);if rig.guest.gets.len()>gets { break; } }
        assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(&b"Windows before kill"[..]));
        if marked {
            let share=rig.h.share.as_mut().unwrap();share.mark_put("doc.bin",rig.now);
            if let Some(file)=share.sync_file.as_ref() { file.wait_marks(); }
        }
        if clean {
            let share=rig.h.share.as_mut().unwrap();
            share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
            assert!(std::fs::read_to_string(format!("{}.share-sync",vars.display())).unwrap().contains("session unclean"),"recovery PUT not confirmed but session marked clean");
        }
        let paths=[".share-sync",".share-sync-puts"].map(|suffix| PathBuf::from(format!("{}{suffix}",vars.display())));
        let disk=paths.each_ref().map(|p| std::fs::read(p).ok());rig.h.share=None;
        for (p,bytes) in paths.iter().zip(disk) { if let Some(bytes)=bytes { std::fs::write(p,bytes).unwrap(); }else{let _=std::fs::remove_file(p);} }
        new_session(&mut rig,Some(&vars));rig.run(24);
        assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));
        assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(&b"Windows before kill"[..]));
        assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
    }
}

#[test]
fn unclean_first_listing_keeps_mac_and_windows_versions() {
    for empty in [false, true] {
        let (mut rig, vars) = synced_from_guest(if empty { "unclean-empty" } else { "unclean-edit" }, now_ms()-2*HOUR_MS, true);
        settle(&mut rig);
        let good = rig.host("doc.bin").unwrap();
        rig.guest.write("doc.bin", if empty { b"" } else { b"recovered Windows bytes" });
        rig.guest.stamps.insert("DOC.BIN".into(), now_ms());
        rig.h.share = None; // Drop is not a clean guest shutdown.
        new_session(&mut rig, Some(&vars));
        rig.run(24);
        assert_eq!(rig.host("doc.bin"), Some(good.clone()), "unclean guest version replaced Mac");
        if !empty {
            assert_eq!(rig.host("doc (Windows copy).bin").as_deref(), Some(&b"recovered Windows bytes"[..]));
            assert_eq!(rig.guest.content("doc (Windows copy).bin"), Some(&b"recovered Windows bytes"[..]));
        } else { assert!(rig.host("doc (Windows copy).bin").is_none()); }
        assert_eq!(rig.guest.content("doc.bin"), Some(good.as_slice()));
    }
}

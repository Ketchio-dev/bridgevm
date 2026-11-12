use super::*;

#[test]
fn state_lifecycle_old_missing_kill_restart_and_clean_end() {
    let mut rig = Rig::new("unclean-state");
    let vars = vars(&rig);
    let state = PathBuf::from(format!("{}.share-sync", vars.display()));
    let (file, _, _, error) = sync_file::SyncStateFile::open(&vars, &rig.dir);
    assert!(error.is_none());
    assert!(file.previous_unclean, "missing state must be unclean");
    assert!(std::fs::read_to_string(&state).unwrap().contains("session unclean"), "set before any mutation");
    drop(file); // simulated kill: no finish
    let (file, _, _, _) = sync_file::SyncStateFile::open(&vars, &rig.dir);
    assert!(file.previous_unclean);
    drop(file);
    let mut text = std::fs::read_to_string(&state).unwrap().replace("bridgevm-share-sync 2", "bridgevm-share-sync 1");
    text = text.lines().filter(|l| !l.starts_with("session ")).collect::<Vec<_>>().join("\n")+"\n";
    std::fs::write(&state, text).unwrap();
    let (mut file, _, _, error) = sync_file::SyncStateFile::open(&vars, &rig.dir);
    assert!(error.is_none()); assert!(file.previous_unclean, "old format is unclean");
    file.finish_clean(&mut ShareSync::new(512));
    let (file, _, _, _) = sync_file::SyncStateFile::open(&vars, &rig.dir);
    assert!(!file.previous_unclean, "explicit clean end is recognized");
    drop(file);
    let mut share = ShareState::new(ShareSync::new(512),rig.dir.clone(),ROOT.into(),Duration::from_secs(1));
    share.attach_sync_state(&vars); rig.h.share=Some(share);
    rig.h.resync_after_agent_restart("GUEST v3-share2",rig.now);
    assert!(std::fs::read_to_string(&state).unwrap().contains("session unclean"));
    rig.h.share=None; // same Drop path used for guest reset; never clears
    assert!(std::fs::read_to_string(&state).unwrap().contains("session unclean"));
}

#[test]
fn clean_first_listing_still_accepts_windows_edit() {
    let (mut rig, vars) = synced_from_guest("clean-edit",now_ms()-2*HOUR_MS,true);
    settle(&mut rig);
    rig.guest.write("doc.bin",b"legitimate Windows edit");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms());
    new_session(&mut rig,Some(&vars));
    rig.run(24);
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"legitimate Windows edit"[..]));
    assert!(rig.host("doc (Windows copy).bin").is_none());
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

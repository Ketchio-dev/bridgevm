//! A Windows edit made while the last transfer of its file is still settling (SYNC_SETTLE) must
//! survive however the session ends: fetched in the same session, fetched after a clean
//! power-off, and kept beside the Mac copy as "(Windows copy)" after a Force Stop, a crash or a
//! guest restart (#648), when the host cannot tell the edit from a copy Windows lost in its cache.
use super::*;
#[path = "share_clean_marks_tests.rs"]
mod clean_marks;

const EDIT: &[u8] = b"genuine later Windows edit, different size";

fn baseline(label: &str, from_guest: bool) -> (Rig, PathBuf) {
    if from_guest { return synced_from_guest(label, now_ms()-HOUR_MS, true); }
    let mut rig=Rig::new(label);
    let vars=vars(&rig);new_session(&mut rig,Some(&vars));
    std::fs::write(rig.dir.join("doc.bin"),b"old version").unwrap();
    // Stop immediately after confirmation, not after the settling window.
    for _ in 0..16 {
        rig.run(1);
        if rig.guest.content("doc.bin")==Some(&b"old version"[..]) { break; }
    }
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
    assert!(rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    rig.run(4); // allow normal state flush while remaining well inside SYNC_SETTLE
    assert!(rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    (rig,vars)
}

fn edit(rig: &mut Rig) {
    rig.guest.write("doc.bin",EDIT);
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms()+HOUR_MS);
}

fn survives(rig: &Rig) -> bool {
    rig.guest.content("doc.bin")==Some(EDIT)
        || rig.host("doc.bin").as_deref()==Some(EDIT)
        || rig.host_names().iter().any(|name| name.contains("Windows copy") && rig.host(name).as_deref()==Some(EDIT))
}

fn running(from_guest: bool) {
    let (mut rig,_vars)=baseline(if from_guest {"settle-A-get"}else{"settle-A-put"},from_guest);
    assert!(rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    edit(&mut rig);
    let gets=rig.guest.gets.len();let puts=rig.guest.puts.len();
    rig.run(8);
    println!("RUNNING origin_get={from_guest} before_settle gets={} puts={} host_edit={} guest_edit={} marked={}",rig.guest.gets.len()-gets,rig.guest.puts.len()-puts,rig.host("doc.bin").as_deref()==Some(EDIT),rig.guest.content("doc.bin")==Some(EDIT),rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    assert!(survives(&rig),"A Windows edit lost in same session before mark settled");
    rig.run(28);
    assert!(survives(&rig),"A Windows edit lost after mark settled");
    assert_eq!(rig.host("doc.bin").as_deref(),Some(EDIT),"A edit never fetched after settle");
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stop {
    /// Observed guest power-off: the share state finishes clean.
    Clean,
    /// Force Stop or a crash: the state files exactly as they were, no finishing write.
    Kill,
    /// Guest restart: the session ends without a power-off (its finishing writes do happen).
    Reset,
}

fn restart(from_guest: bool, stop: Stop) {
    let label=format!("settle-{stop:?}-{}",if from_guest {"get"}else{"put"});
    let (mut rig,vars)=baseline(&label,from_guest);
    edit(&mut rig);
    let paths=[".share-sync",".share-sync-puts"].map(|s|PathBuf::from(format!("{}{s}",vars.display())));
    if stop==Stop::Clean {
        let share=rig.h.share.as_mut().unwrap();
        share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
    }
    let disk=paths.each_ref().map(|p|std::fs::read(p).ok());
    println!("RESTART stop={stop:?} origin_get={from_guest} state={:?}",disk.each_ref().map(|b|b.as_ref().map(|b|String::from_utf8_lossy(b).into_owned())));
    rig.h.share=None;
    if stop!=Stop::Reset {
        // For kill, restore exact files before Drop's finishing writes; clean uses the actual finish.
        for (p,bytes) in paths.iter().zip(disk) {if let Some(bytes)=bytes {std::fs::write(p,bytes).unwrap();}else{let _=std::fs::remove_file(p);}}
    }
    new_session(&mut rig,Some(&vars));
    let gets=rig.guest.gets.len();let puts=rig.guest.puts.len();rig.run(24);
    println!("AFTER stop={stop:?} origin_get={from_guest} gets={} puts={} host_edit={} guest_edit={}",rig.guest.gets.len()-gets,rig.guest.puts.len()-puts,rig.host("doc.bin").as_deref()==Some(EDIT),rig.guest.content("doc.bin")==Some(EDIT));
    assert!(survives(&rig),"{stop:?}: Windows edit overwritten after restart with settling mark");
    if stop!=Stop::Clean {
        // Edit or cache damage cannot be told apart: the Mac copy stays, the Windows one goes beside it.
        assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]),"{stop:?}: Mac copy replaced");
        assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(EDIT),"{stop:?}: Windows version not kept beside the Mac copy");
        assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
    }
}

/// The versions are written about a second after a fetched copy's mark: a kill in between leaves
/// the mark with no synced version for the name (seen when the tests above ran in parallel).
#[test]
fn settling_get_force_stop_before_its_versions_are_written_keeps_both() {
    use std::sync::atomic::Ordering;
    let mut rig=Rig::new("settle-versions-lag");
    let vars=vars(&rig);new_session(&mut rig,Some(&vars));
    rig.run(4);
    let marks=rig.h.share.as_ref().and_then(ShareState::sync_marks).unwrap();
    marks.hold.store(true,Ordering::Release); // the state thread's versions write waits
    rig.guest.write("doc.bin",b"old version");
    rig.guest.stamps.insert("DOC.BIN".into(),now_ms()-HOUR_MS);
    for _ in 0..16 { rig.run(1); if rig.host("doc.bin").is_some() { break; } }
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));
    edit(&mut rig);
    let paths=[".share-sync",".share-sync-puts"].map(|s|PathBuf::from(format!("{}{s}",vars.display())));
    let disk=paths.each_ref().map(|p|std::fs::read_to_string(p).ok());
    assert!(!disk[0].as_deref().unwrap_or_default().contains("\nfile "),"versions already written: {disk:?}");
    assert!(disk[1].as_deref().unwrap_or_default().contains("\nmark "),"no mark on disk: {disk:?}");
    marks.hold.store(false,Ordering::Release);
    rig.h.share=None;
    for (p,text) in paths.iter().zip(disk) {if let Some(text)=text {std::fs::write(p,text).unwrap();}else{let _=std::fs::remove_file(p);}}
    new_session(&mut rig,Some(&vars));rig.run(24);
    assert!(survives(&rig),"Windows edit overwritten after a kill before the versions write");
    assert_eq!(rig.host("doc.bin").as_deref(),Some(&b"old version"[..]));
    assert_eq!(rig.host("doc (Windows copy).bin").as_deref(),Some(EDIT));
}

#[test] fn settling_reproduction_a_put_then_windows_edit_no_stop(){running(false);}
#[test] fn settling_reproduction_a_get_then_windows_edit_no_stop(){running(true);}
#[test] fn settling_reproduction_b_put_then_windows_edit_clean_stop(){restart(false,Stop::Clean);}
#[test] fn settling_reproduction_b_get_then_windows_edit_clean_stop(){restart(true,Stop::Clean);}
#[test] fn settling_c_put_then_windows_edit_force_stop_keeps_both(){restart(false,Stop::Kill);}
#[test] fn settling_c_get_then_windows_edit_force_stop_keeps_both(){restart(true,Stop::Kill);}
#[test] fn settling_put_then_windows_edit_guest_restart_keeps_both(){restart(false,Stop::Reset);}
#[test] fn settling_get_then_windows_edit_guest_restart_keeps_both(){restart(true,Stop::Reset);}

use super::*;

#[test]
fn clean_stop_supersedes_confirmed_marks_after_queued_writer() {
    use std::sync::atomic::Ordering;
    let (mut rig,vars)=baseline("clean-held-writer",true);
    let share=rig.h.share.as_mut().unwrap();
    let file=share.sync_file.as_mut().unwrap();let marks=file.marks();
    marks.hold.store(true,Ordering::Release);
    share.engine.mark_settling("doc.bin",rig.now);
    share.engine.put_confirmed("queued.bin",0,Some(1),rig.now); // force a queued versions snapshot
    file.flush(&mut share.engine,rig.now+Duration::from_secs(2));
    let release=std::thread::spawn(move || {std::thread::sleep(Duration::from_millis(30));marks.hold.store(false,Ordering::Release);});
    file.finish_clean(&mut share.engine);release.join().unwrap();
    assert!(!share.engine.marked("doc.bin"));
    let versions=std::fs::read_to_string(format!("{}.share-sync",vars.display())).unwrap();
    let puts=std::fs::read_to_string(format!("{}.share-sync-puts",vars.display())).unwrap();
    assert!(!versions.lines().any(|l|l.starts_with("mark ")));
    assert!(versions.contains("session clean"));
    let seq=|s:&str| s.lines().find_map(|l|l.strip_prefix("seq ")).unwrap().parse::<u64>().unwrap();
    assert!(seq(&versions)>seq(&puts),"clean snapshot did not supersede stale marks");
    rig.h.share=None;new_session(&mut rig,Some(&vars));
    assert!(!rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
}

#[test]
fn clean_stop_keeps_unconfirmed_put_mark_and_resends_mac() {
    let (mut rig,vars)=baseline("clean-unconfirmed",false);
    let share=rig.h.share.as_mut().unwrap();share.mark_put("doc.bin",rig.now);
    share.sync_file.as_ref().unwrap().wait_marks();
    edit(&mut rig);new_session(&mut rig,Some(&vars));
    assert!(rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    rig.run(24);
    assert_eq!(rig.guest.content("doc.bin"),Some(&b"old version"[..]));
}

#[test]
fn failed_retirement_write_keeps_old_disk_protection() {
    for only_in_puts in [false,true] {
        let (mut rig,vars)=baseline(if only_in_puts {"retire-only-puts-fail"}else{"retire-snapshot-fail"},true);
        let share=rig.h.share.as_mut().unwrap();
        // Complete current snapshot first, then optionally strip its mark to model marks-only state.
        share.sync_file.as_mut().unwrap().finish(&mut share.engine);
        let path=PathBuf::from(format!("{}.share-sync",vars.display()));
        if only_in_puts {
            let text=std::fs::read_to_string(&path).unwrap();
            let seq=text.lines().find(|l|l.starts_with("seq ")).unwrap().to_owned();
            let text=text.lines().filter(|l|!l.starts_with("mark ")).collect::<Vec<_>>().join("\n")+"\n";
            std::fs::write(&path,text.replace(&seq,"seq 0")).unwrap();
        }
        let immutable=super::super::Mutable::set(&path);
        let share=rig.h.share.as_mut().unwrap();share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
        let text=std::fs::read_to_string(format!("{}.share-sync",vars.display())).unwrap();
        assert!(text.contains("session unclean"));
        drop(immutable);rig.h.share=None;new_session(&mut rig,Some(&vars));
        assert!(rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    }
}

#[test]
fn marks_only_clean_retirement_survives_reopen_and_next_mark_write() {
    let (mut rig,vars)=baseline("retire-marks-only",true);
    let share=rig.h.share.as_mut().unwrap();
    share.sync_file.as_mut().unwrap().finish(&mut share.engine);
    let path=PathBuf::from(format!("{}.share-sync",vars.display()));
    let text=std::fs::read_to_string(&path).unwrap();
    let seq=text.lines().find(|l|l.starts_with("seq ")).unwrap().to_owned();
    let text=text.lines().filter(|l|!l.starts_with("mark ")).collect::<Vec<_>>().join("\n")+"\n";
    std::fs::write(&path,text.replace(&seq,"seq 0")).unwrap();
    share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
    let stale=std::fs::read_to_string(format!("{}.share-sync-puts",vars.display())).unwrap();
    assert!(stale.contains("mark ZG9jLmJpbg=="));
    rig.h.share=None;new_session(&mut rig,Some(&vars));
    let share=rig.h.share.as_mut().unwrap();
    assert!(!share.sync_file.as_ref().unwrap().previous_unclean);
    assert!(!share.engine.marked("doc.bin"));
    share.mark_put("other.bin",rig.now);share.sync_file.as_ref().unwrap().wait_marks();
    let rewritten=std::fs::read_to_string(format!("{}.share-sync-puts",vars.display())).unwrap();
    assert!(!rewritten.contains("mark ZG9jLmJpbg=="),"next mark resurrected retired name");
    edit(&mut rig);rig.run(24);
    assert_eq!(rig.host("doc.bin").as_deref(),Some(EDIT));
}

#[test]
fn unexamined_unclean_stop_keeps_settling_mark() {
    let (mut rig,vars)=baseline("not-clean-mark",true);
    rig.h.share=None;new_session(&mut rig,Some(&vars));
    let share=rig.h.share.as_mut().unwrap();share.engine.mark_settling("doc.bin",rig.now);
    share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
    assert!(share.engine.marked("doc.bin"));
    assert!(std::fs::read_to_string(format!("{}.share-sync",vars.display())).unwrap().contains("session unclean"));
}

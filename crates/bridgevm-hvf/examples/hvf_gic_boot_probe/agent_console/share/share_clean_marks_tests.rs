use super::*;

#[test]
fn clean_stop_retires_confirmed_marks_in_both_files_after_queued_writer() {
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
    for suffix in [".share-sync",".share-sync-puts"] {
        let text=std::fs::read_to_string(format!("{}{suffix}",vars.display())).unwrap();
        assert!(!text.lines().any(|l|l.starts_with("mark ")),"retired mark survived in {suffix}");
        if suffix==".share-sync" {assert!(text.contains("session clean"));}
    }
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
    for suffix in [".share-sync-puts",".share-sync"] {
        let (mut rig,vars)=baseline(if suffix==".share-sync" {"retire-versions-fail"}else{"retire-marks-fail"},true);
        let immutable=super::super::Mutable::set(&PathBuf::from(format!("{}{suffix}",vars.display())));
        let share=rig.h.share.as_mut().unwrap();share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
        let text=std::fs::read_to_string(format!("{}.share-sync",vars.display())).unwrap();
        assert!(text.contains("session unclean"));assert!(text.contains("mark "),"failed retirement lost old versions protection");
        drop(immutable);rig.h.share=None;new_session(&mut rig,Some(&vars));
        assert!(rig.h.share.as_ref().unwrap().engine.marked("doc.bin"));
    }
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

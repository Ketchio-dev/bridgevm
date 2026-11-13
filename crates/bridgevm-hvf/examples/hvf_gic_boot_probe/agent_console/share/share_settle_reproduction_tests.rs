//! Reading/reproduction only: these assertions deliberately expose loss if main loses an edit.
use super::*;

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

fn restart(from_guest: bool, clean: bool) {
    let label=format!("settle-{}-{}",if clean {"B"}else{"C"},if from_guest {"get"}else{"put"});
    let (mut rig,vars)=baseline(&label,from_guest);
    edit(&mut rig);
    let paths=[".share-sync",".share-sync-puts"].map(|s|PathBuf::from(format!("{}{s}",vars.display())));
    if clean {
        let share=rig.h.share.as_mut().unwrap();
        share.sync_file.as_mut().unwrap().finish_clean(&mut share.engine);
    }
    let disk=paths.each_ref().map(|p|std::fs::read(p).ok());
    println!("RESTART clean={clean} origin_get={from_guest} state={:?}",disk.each_ref().map(|b|b.as_ref().map(|b|String::from_utf8_lossy(b).into_owned())));
    rig.h.share=None;
    // For kill, restore exact files before Drop's finishing writes; clean uses the actual finish.
    for (p,bytes) in paths.iter().zip(disk) {if let Some(bytes)=bytes {std::fs::write(p,bytes).unwrap();}else{let _=std::fs::remove_file(p);}}
    new_session(&mut rig,Some(&vars));
    let gets=rig.guest.gets.len();let puts=rig.guest.puts.len();rig.run(24);
    println!("AFTER clean={clean} origin_get={from_guest} gets={} puts={} host_edit={} guest_edit={}",rig.guest.gets.len()-gets,rig.guest.puts.len()-puts,rig.host("doc.bin").as_deref()==Some(EDIT),rig.guest.content("doc.bin")==Some(EDIT));
    assert!(survives(&rig),"B/C Windows edit overwritten after restart with settling mark");
}

#[test] fn settling_reproduction_a_put_then_windows_edit_no_stop(){running(false);}
#[test] fn settling_reproduction_a_get_then_windows_edit_no_stop(){running(true);}
#[test] fn settling_reproduction_b_put_then_windows_edit_clean_stop(){restart(false,true);}
#[test] fn settling_reproduction_b_get_then_windows_edit_clean_stop(){restart(true,true);}
#[test] fn settling_reproduction_c_put_then_windows_edit_kill(){restart(false,false);}
#[test] fn settling_reproduction_c_get_then_windows_edit_kill(){restart(true,false);}

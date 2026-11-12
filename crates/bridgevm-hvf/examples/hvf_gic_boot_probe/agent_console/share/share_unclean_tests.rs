use super::*;

#[test]
fn unclean_first_listing_keeps_mac_and_windows_versions() {
    for empty in [false, true] {
        let (mut rig, vars) = synced_from_guest(if empty { "unclean-empty" } else { "unclean-edit" }, now_ms()-2*HOUR_MS, true);
        settle(&mut rig);
        let good = rig.host("doc.bin").unwrap();
        rig.guest.write("doc.bin", if empty { b"" } else { b"recovered Windows bytes" });
        rig.guest.stamps.insert("DOC.BIN".into(), now_ms());
        new_session(&mut rig, Some(&vars)); // Drop is not a clean guest shutdown.
        rig.run(24);
        assert_eq!(rig.host("doc.bin"), Some(good.clone()), "unclean guest version replaced Mac");
        if !empty {
            assert_eq!(rig.host("doc (Windows copy).bin").as_deref(), Some(&b"recovered Windows bytes"[..]));
            assert_eq!(rig.guest.content("doc (Windows copy).bin"), Some(&b"recovered Windows bytes"[..]));
        } else { assert!(rig.host("doc (Windows copy).bin").is_none()); }
        assert_eq!(rig.guest.content("doc.bin"), Some(good.as_slice()));
    }
}

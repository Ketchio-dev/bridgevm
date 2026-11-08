//! Mac names as Windows stores and shows them (share_guest_names.rs), through
//! the service and the modelled NTFS guest, which keeps code points as given.

use super::*;

/// "한글.txt" as Foundation writes it (NFD).
const NFD: &str = "\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}.txt";
const NFC: &str = "한글.txt";

#[test]
fn a_decomposed_mac_name_reaches_windows_composed_and_settles() {
    let mut rig = Rig::new("names-nfd");
    std::fs::write(rig.dir.join(NFD), b"mac").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content(NFC), Some(&b"mac"[..]), "{:?}", rig.guest.puts);
    assert_eq!(rig.guest.content(NFD), None);
    rig.assert_settled();
    std::fs::write(rig.dir.join(NFD), b"mac 2").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content(NFC), Some(&b"mac 2"[..]));
    rig.guest.write(NFC, b"guest");
    rig.run(12);
    assert_eq!(rig.host(NFD).as_deref(), Some(&b"guest"[..]));
    assert_eq!(rig.host_names().len(), 1, "{:?}", rig.host_names());
    rig.assert_settled();
}

#[test]
fn characters_windows_refuses_travel_as_stand_ins_both_ways() {
    let mac = "2026:11:08 why?.txt";
    let guest = "2026\u{F022}11\u{F022}08 why\u{F025}.txt";
    let mut rig = Rig::new("names-pua");
    std::fs::write(rig.dir.join(mac), b"mac").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content(guest), Some(&b"mac"[..]), "{:?}", rig.guest.puts);
    rig.assert_settled();
    rig.guest.write(guest, b"guest");
    rig.guest.write("a\u{F023}b\u{F024}.txt", b"new in windows");
    rig.run(12);
    assert_eq!(rig.host(mac).as_deref(), Some(&b"guest"[..]));
    assert_eq!(rig.host("a<b>.txt").as_deref(), Some(&b"new in windows"[..]));
    rig.assert_settled();
}

#[test]
fn a_decomposed_windows_name_syncs_and_mac_edits_go_back_into_that_file() {
    let mut rig = Rig::new("names-guest-nfd");
    rig.guest.write(NFD, b"from windows");
    rig.run(12);
    assert_eq!(rig.host(NFC).as_deref(), Some(&b"from windows"[..]));
    std::fs::write(rig.dir.join(NFC), b"edited on the mac").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content(NFD), Some(&b"edited on the mac"[..]), "{:?}", rig.guest.puts);
    assert_eq!(rig.guest.files.len(), 1, "a composed twin was written");
    rig.assert_settled();
}

#[test]
fn when_both_forms_are_listed_the_composed_copy_is_the_file() {
    for decomposed_first in [true, false] {
        let mut rig = Rig::new("names-twins");
        if decomposed_first {
            rig.guest.write(NFD, b"old decomposed copy");
        }
        rig.guest.write(NFC, b"composed copy");
        if !decomposed_first {
            rig.guest.write(NFD, b"old decomposed copy");
        }
        rig.run(12);
        assert_eq!(rig.host(NFC).as_deref(), Some(&b"composed copy"[..]), "decomposed first: {decomposed_first}");
        rig.assert_settled();
        std::fs::write(rig.dir.join(NFC), b"mac edit").unwrap();
        rig.run(12);
        assert_eq!(rig.guest.content(NFC), Some(&b"mac edit"[..]));
        assert_eq!(rig.guest.content(NFD), Some(&b"old decomposed copy"[..]));
        rig.assert_settled();
        rig.guest.write(NFC, b"windows edit");
        rig.run(12);
        assert_eq!(rig.host(NFC).as_deref(), Some(&b"windows edit"[..]), "decomposed first: {decomposed_first}");
        rig.assert_settled();
    }
}

#[test]
fn a_new_mac_file_goes_into_the_decomposed_windows_folder_it_belongs_to() {
    // "사진" decomposed, as a Mac archive unpacked in Windows leaves it.
    let folder = "\u{1109}\u{1161}\u{110C}\u{1175}\u{11AB}";
    let mut rig = Rig::new("names-folder");
    rig.guest.write(&format!("{folder}\\old.txt"), b"from windows");
    rig.run(12);
    assert_eq!(rig.host("사진/old.txt").as_deref(), Some(&b"from windows"[..]));
    std::fs::write(rig.dir.join("사진/new.txt"), b"from the mac").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content(&format!("{folder}\\new.txt")), Some(&b"from the mac"[..]), "{:?}", rig.guest.puts);
    assert_eq!(rig.guest.files.len(), 2, "a composed twin folder was made");
    rig.assert_settled();
}

#[test]
fn a_mac_name_holding_a_stand_in_character_stays_out_and_is_never_renamed() {
    let mut rig = Rig::new("names-literal");
    std::fs::write(rig.dir.join("why\u{F025}.txt"), b"literal").unwrap();
    std::fs::write(rig.dir.join("why?.txt"), b"question").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content("why\u{F025}.txt"), Some(&b"question"[..]), "{:?}", rig.guest.puts);
    assert_eq!(rig.guest.files.len(), 1);
    rig.assert_settled();
    assert_eq!(rig.host("why\u{F025}.txt").as_deref(), Some(&b"literal"[..]));
    assert_eq!(rig.host("why?.txt").as_deref(), Some(&b"question"[..]));
}

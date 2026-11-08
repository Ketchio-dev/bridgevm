use super::*;

#[test]
fn a_decomposed_mac_name_is_composed_and_a_composed_one_is_left_alone() {
    // "보고서 한글.txt" as Foundation writes it (NFD), then as typed (NFC).
    let nfd = "\u{1107}\u{1169}\u{1100}\u{1169}\u{1109}\u{1165} \u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}.txt";
    assert_eq!(composed(nfd), "보고서 한글.txt");
    assert!(matches!(composed("보고서 한글.txt"), Cow::Borrowed(_)));
    assert_eq!(composed("cafe\u{301} \u{304B}\u{3099}.txt"), "café が.txt");
    assert!(matches!(composed("plain name.txt"), Cow::Borrowed(_)));
}

#[test]
fn characters_windows_refuses_travel_as_private_use_and_come_back() {
    let mac = "2026:11:08 why? <a|b> \"*\".txt";
    let guest: String = mac.chars().map(guest_char).collect();
    assert!(!guest.contains(['"', '*', ':', '<', '>', '?', '|']), "{guest}");
    assert_eq!(key_from_guest(&guest), mac);
    // A decomposed Windows name maps to the composed key.
    assert_eq!(key_from_guest("\u{1112}\u{1161}\u{11AB}\u{F025}.txt"), "한?.txt");
    assert!(matches!(key_from_guest("한글.txt"), Cow::Borrowed(_)));
}

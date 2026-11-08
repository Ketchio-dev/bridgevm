//! Finder file copies to Windows (CLIPSETFILES) through the resident service
//! loop: PUTs and CLIPSETFILES against the Rig's Mac and guest
//! (clip_image_tests.rs).

use super::*;
use std::path::Path;

/// A service whose agent announced CLIPSETFILES, with its startup traffic cleared.
fn rig() -> Rig {
    let mut rig = Rig::unsettled(true);
    rig.guest.html_caps = true;
    rig.guest.files_caps = true;
    rig.guest.set_files_caps = true;
    rig.settle();
    assert!(rig.h.clip_files_push);
    rig.verbs();
    take_lines(&mut Vec::new());
    rig
}

/// A fresh Mac folder for one test's copied items.
fn mac_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bridgevm-files-push-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A file and a folder holding a file, an empty folder and a link to the file.
fn two_items(dir: &Path) -> Vec<PathBuf> {
    let (file, folder) = (dir.join("보고서.txt"), dir.join("a folder"));
    std::fs::write(&file, b"report").unwrap();
    std::fs::create_dir_all(folder.join("empty")).unwrap();
    std::fs::write(folder.join("inner.txt"), b"in").unwrap();
    std::os::unix::fs::symlink(&file, folder.join("link.txt")).unwrap();
    vec![file, folder]
}

fn b64(text: &str) -> String {
    base64_encode(text.as_bytes())
}

/// The guest paths of the PUTBEGs on the wire.
fn put_paths(wire: &[String]) -> Vec<String> {
    wire.iter()
        .filter_map(|line| line.strip_prefix("PUTBEG "))
        .map(|rest| Guest::b64_text(rest.split(' ').next().unwrap()))
        .collect()
}

/// The guest answers `line`; what the service sends next.
fn answer(rig: &mut Rig, line: &[u8]) -> Vec<Vec<u8>> {
    let mut sent = Vec::new();
    for reply in rig.guest.answer(std::str::from_utf8(line).unwrap()) {
        rig.h.handle_service_reply_with_sender(&reply, rig.now, &mut |bytes| sent.push(bytes.to_vec()));
    }
    sent
}

/// The Mac copy goes out within this clip interval; its first wire line.
fn start_push(rig: &mut Rig) -> Vec<u8> {
    assert!(rig.worker.poll(&mut rig.mac));
    rig.h.service_enqueue(rig.now);
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |bytes| sent.push(bytes.to_vec()));
    assert_eq!(sent.len(), 1);
    sent.remove(0)
}

#[test]
fn a_finder_copy_goes_as_its_puts_then_one_clipsetfiles_and_never_comes_back() {
    let mut rig = rig();
    let dir = mac_dir("two");
    rig.mac.copy_files("보고서.txt\na folder", two_items(&dir));
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.guest.set_files.len(), 1, "one CLIPSETFILES");
    let folder = rig.guest.set_files[0].0.clone();
    assert!(folder.len() <= 19 && folder.bytes().all(|byte| byte.is_ascii_digit()), "{folder}");
    let root = format!(r"{DIR}\files\{folder}");
    let put = |rel: &str, bytes: &[u8]| {
        [
            format!("PUTBEG {} {} 1", b64(&format!(r"{root}\{rel}")), bytes.len()),
            format!("PUTCHUNK 0 {}", base64_encode(bytes)),
            "PUTEND 1".to_owned(),
        ]
    };
    let list = "0|보고서.txt\n1|a folder\n1|a folder\\empty\n0|a folder\\inner.txt\n";
    let mut expected = [put("보고서.txt", b"report"), put(r"a folder\inner.txt", b"in")].concat();
    expected.push(format!("CLIPSETFILES {folder} {} {}", b64(list), b64("보고서.txt\r\na folder")));
    let wire: Vec<String> = rig.wire.drain(..).filter(|line| line != "CLIPXF").collect();
    assert_eq!(wire, expected, "each file PUT (not the link), then CLIPSETFILES");
    assert_eq!(rig.guest.text, "보고서.txt\na folder", "the names go as its text");
    assert_eq!(rig.guest.files[&format!(r"{root}\보고서.txt")], b"report");
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC host->guest files=4 bytes=8 copy=a"]);
    // The guest's CLIPXF now lists the pushed files: never pulled back.
    let count = rig.mac.count;
    rig.settle();
    rig.settle();
    assert_eq!(rig.verbs(), Vec::<String>::new(), "no GET and no second push");
    assert!(rig.mac.file_writes.is_empty());
    assert_eq!(rig.mac.count, count, "the Mac pasteboard is untouched");
    assert_eq!(take_lines(&mut Vec::new()), Vec::<String>::new());
}

#[test]
fn a_copy_over_the_limits_goes_as_its_names_without_files_or_icon() {
    let mut rig = rig();
    let dir = mac_dir("many");
    let many = dir.join("many");
    std::fs::create_dir(&many).unwrap();
    for index in 0..CLIP_FILES_MAX_ENTRIES {
        std::fs::write(many.join(format!("{index}.txt")), b"").unwrap();
    }
    rig.mac.copy_files("many", vec![many]);
    rig.mac.image = Some(PasteboardImage::Png(png(32, 32, 0, 1)));
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.verbs(), verbs(&["CLIPSET"]));
    assert_eq!((rig.guest.text.as_str(), rig.guest.image.as_ref()), ("many", None));
    assert_eq!(
        take_lines(&mut Vec::new()),
        [
            "BVAGENT CLIPSYNC skip host-files too-large files=257 bytes=0 copy=a",
            "BVAGENT CLIPSYNC host->guest bytes=4 copy=a",
        ]
    );
}

#[test]
fn a_newer_mac_copy_mid_push_ends_it_and_goes_alone() {
    let mut rig = rig();
    let dir = mac_dir("newer");
    rig.mac.copy_files("보고서.txt\na folder", two_items(&dir));
    let putbeg = start_push(&mut rig);
    let chunk = answer(&mut rig, &putbeg);
    assert!(chunk[0].starts_with(b"PUTCHUNK 0 "));
    rig.mac.copy(Some("newer"), None);
    assert!(rig.worker.poll(&mut rig.mac));
    rig.h.service_enqueue(rig.now);
    assert!(answer(&mut rig, &chunk[0]).is_empty(), "the push ends at its next reply");
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.verbs(), verbs(&["CLIPSET"]));
    assert!(rig.guest.set_files.is_empty());
    assert_eq!(rig.guest.text, "newer");
    // The files copy printed nothing: the only copy named is the newer one.
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC host->guest bytes=5 copy=a"]);
}

#[test]
fn agent_restart_mid_push_starts_it_over_from_the_first_file() {
    let mut rig = rig();
    let dir = mac_dir("restart");
    rig.mac.copy_files("보고서.txt\na folder", two_items(&dir));
    let mut line = start_push(&mut rig);
    // The first file in, the second one begun.
    for _ in 0..4 {
        line = answer(&mut rig, &line).remove(0);
    }
    assert!(line.starts_with(b"PUTCHUNK 0 "));
    rig.h.handle_service_reply_with_sender("READY GUEST v3-share2", rig.now, &mut |_| {});
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    let root = format!(r"{DIR}\files\{}", rig.guest.set_files[0].0);
    assert_eq!(put_paths(&rig.wire), [format!(r"{root}\보고서.txt"), format!(r"{root}\a folder\inner.txt")]);
    assert!(rig.verbs().starts_with(&verbs(&["CLIPCAPS", "PUTBEG"])));
    assert_eq!(rig.guest.set_files.len(), 1);
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC host->guest files=4 bytes=8 copy=a"]);
}

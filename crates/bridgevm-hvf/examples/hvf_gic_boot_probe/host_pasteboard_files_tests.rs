//! The walk below a Mac file copy, and the poller's file copy reports.

use super::super::{HostClip, PasteboardBackend, PasteboardImage, Poller};
use super::*;
use std::os::unix::fs::symlink;

/// A fresh directory of this test's own.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bridgevm-host-files-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn entries(files: Option<HostFiles>) -> Vec<(String, Option<Vec<u8>>)> {
    match files {
        Some(HostFiles::Read(copy)) => copy.entries.into_iter().map(|entry| (entry.rel, entry.data)).collect(),
        other => panic!("not read: {other:?}"),
    }
}

#[test]
fn a_copy_is_its_items_and_their_tree_without_links_or_names_windows_cannot_hold() {
    let dir = scratch("tree");
    let outside = dir.join("outside");
    std::fs::create_dir_all(outside.join("dir")).unwrap();
    std::fs::write(outside.join("secret.txt"), b"secret").unwrap();
    let copied = dir.join("copied");
    // "보고서.txt" as Foundation writes it (NFD), and a name with a `:`.
    let file = copied.join("\u{1107}\u{1169}\u{1100}\u{1169}\u{1109}\u{1165}.txt");
    let folder = copied.join("a folder");
    std::fs::create_dir_all(folder.join("empty")).unwrap();
    std::fs::write(&file, b"report").unwrap();
    std::fs::write(folder.join("inner:1.txt"), b"in").unwrap();
    std::fs::write(folder.join("Icon\r"), b"icon").unwrap();
    symlink(outside.join("secret.txt"), folder.join("link.txt")).unwrap();
    symlink(outside.join("dir"), folder.join("link dir")).unwrap();
    let fifo = std::ffi::CString::new(folder.join("pipe").into_os_string().into_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    symlink(outside.join("secret.txt"), copied.join("top link")).unwrap();
    let tops = [file, folder, copied.join("top link"), copied.join("gone.txt")];
    assert_eq!(
        entries(host_files(&tops)),
        [
            ("보고서.txt".to_owned(), Some(b"report".to_vec())),
            ("a folder".to_owned(), None),
            ("a folder\\empty".to_owned(), None),
            ("a folder\\inner\u{F022}1.txt".to_owned(), Some(b"in".to_vec())),
        ]
    );
    assert_eq!(host_files(&[copied.join("top link"), copied.join("gone.txt")]), None, "no file, no copy");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_name_holding_a_windows_separator_is_left_out() {
    let dir = scratch("separator");
    // Legal in a Mac name; in a Windows path it would climb out of the copy.
    let evil = dir.join(r"..\..\victim.txt");
    std::fs::write(&evil, b"evil").unwrap();
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join(r"..\up.txt"), b"up").unwrap();
    std::fs::write(folder.join("ok.txt"), b"ok").unwrap();
    assert_eq!(
        entries(host_files(&[evil, folder])),
        [("folder".to_owned(), None), ("folder\\ok.txt".to_owned(), Some(b"ok".to_vec()))]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn copied_items_of_one_name_get_numbered_as_explorer_does() {
    let dir = scratch("same-name");
    let mut tops = Vec::new();
    for (sub, name) in [("a", "a.txt"), ("b", "a.txt"), ("c", "A.txt"), ("d", "photos"), ("e", "photos")] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        let path = dir.join(sub).join(name);
        if name == "photos" {
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join("one.jpg"), sub.as_bytes()).unwrap();
        } else {
            std::fs::write(&path, sub.as_bytes()).unwrap();
        }
        tops.push(path);
    }
    assert_eq!(
        entries(host_files(&tops)),
        [
            ("a.txt".to_owned(), Some(b"a".to_vec())),
            ("a (2).txt".to_owned(), Some(b"b".to_vec())),
            ("A (3).txt".to_owned(), Some(b"c".to_vec())),
            ("photos".to_owned(), None),
            ("photos\\one.jpg".to_owned(), Some(b"d".to_vec())),
            ("photos (2)".to_owned(), None),
            ("photos (2)\\one.jpg".to_owned(), Some(b"e".to_vec())),
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn names_that_meet_in_windows_inside_a_folder_get_numbered() {
    let dir = scratch("same-child");
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    // `:` goes as U+F022, which a Mac name may already hold.
    std::fs::write(folder.join("a:b.txt"), b"colon").unwrap();
    std::fs::write(folder.join("a\u{F022}b.txt"), b"stand-in").unwrap();
    std::fs::create_dir(folder.join("sub:1")).unwrap();
    std::fs::write(folder.join("sub:1").join("x"), b"x1").unwrap();
    std::fs::create_dir(folder.join("sub\u{F022}1")).unwrap();
    std::fs::write(folder.join("sub\u{F022}1").join("x"), b"x2").unwrap();
    let entries = entries(host_files(&[folder]));
    let rels: Vec<&str> = entries.iter().map(|(rel, _)| rel.as_str()).collect();
    let unique: std::collections::HashSet<String> = rels.iter().map(|rel| rel.to_lowercase()).collect();
    assert_eq!(unique.len(), rels.len(), "two entries share a Windows path: {rels:?}");
    assert_eq!(entries.len(), 7);
    let bytes: std::collections::HashSet<&[u8]> = entries.iter().filter_map(|(_, data)| data.as_deref()).collect();
    assert_eq!(bytes.len(), 4, "a file's bytes were lost");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn names_go_as_win32_keeps_them() {
    let dir = scratch("win32");
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    // Win32 drops trailing dots and spaces, and a device name is no file there.
    for (name, bytes) in [("report.txt", "a"), ("report.txt.", "b"), ("report.txt ", "c"), ("...", "d"), ("nul.txt", "e"), ("Com1", "f"), ("com10", "g")] {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    assert_eq!(
        entries(host_files(&[folder])),
        [
            ("folder".to_owned(), None),
            ("folder\\com10".to_owned(), Some(b"g".to_vec())),
            ("folder\\report.txt".to_owned(), Some(b"a".to_vec())),
            ("folder\\report (2).txt".to_owned(), Some(b"c".to_vec())),
            ("folder\\report (3).txt".to_owned(), Some(b"b".to_vec())),
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_folder_that_cannot_be_listed_fails_the_copy() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("unlisted");
    let folder = dir.join("folder");
    let inner = folder.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("x.txt"), b"x").unwrap();
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o000)).unwrap();
    let files = host_files(&[folder]);
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!matches!(files, Some(HostFiles::Read(_))), "part of the folder read as all of it: {files:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_copy_over_a_limit_is_counted_not_read() {
    let dir = scratch("limits");
    let many = dir.join("many");
    std::fs::create_dir(&many).unwrap();
    for index in 0..CLIP_FILES_MAX_ENTRIES {
        std::fs::write(many.join(format!("{index}.txt")), b"").unwrap();
    }
    assert_eq!(
        host_files(&[many.clone()]),
        Some(HostFiles::TooLarge { entries: CLIP_FILES_MAX_ENTRIES + 1, bytes: 0 }),
        "the folder and its 256 files"
    );
    let big = dir.join("big.bin");
    std::fs::File::create(&big).unwrap().set_len(CLIP_FILES_MAX_BYTES + 1).unwrap();
    assert_eq!(
        host_files(&[big]),
        Some(HostFiles::TooLarge { entries: 1, bytes: CLIP_FILES_MAX_BYTES + 1 })
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A pasteboard with text and file URLs; it counts the image reads (NSPasteboard
/// has no image beside file URLs, read_image).
#[derive(Default)]
struct Finder {
    count: i64,
    text: Option<String>,
    files: Vec<PathBuf>,
    image_reads: usize,
}

impl PasteboardBackend for Finder {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.count += 1;
        (self.text, self.files) = (Some(text.to_owned()), Vec::new());
        true
    }
    fn get_image(&mut self) -> Option<PasteboardImage> {
        self.image_reads += 1;
        None
    }
    fn set_files(&mut self, paths: &[PathBuf]) -> bool {
        self.count += 1;
        (self.text, self.files) = (None, paths.to_vec());
        true
    }
    fn get_files(&mut self) -> Vec<PathBuf> {
        self.files.clone()
    }
}

#[test]
fn a_finder_copy_reports_its_files_and_names_once_never_its_icon_or_our_own_write() {
    let dir = scratch("poller");
    std::fs::write(dir.join("a.txt"), b"a").unwrap();
    let files = vec![dir.join("a.txt")];
    let mut finder = Finder { count: 1, text: Some("a.txt".into()), files, ..Finder::default() };
    let mut poller = Poller::new();
    assert_eq!(poller.on_tick_clip(&mut finder).and_then(|clip| clip.files), None, "file copies off: names only");
    let clip = poller.report_formats_on(&mut finder, true, true, true).expect("switched on: the files");
    assert_eq!(clip.text.as_deref(), Some("a.txt"));
    assert!(clip.image.is_none() && clip.html.is_none());
    assert_eq!(entries(clip.files), [("a.txt".to_owned(), Some(b"a".to_vec()))]);
    assert_eq!(finder.image_reads, 0, "the files win over the icon: never read");
    finder.count += 1;
    assert_eq!(poller.on_tick_clip(&mut finder), None, "the same copy again is a re-copy");
    std::fs::write(dir.join("a.txt"), b"edited").unwrap();
    finder.count += 1;
    assert!(poller.on_tick_clip(&mut finder).is_some_and(|clip| clip.files.is_some()), "new bytes are a new copy");
    // Files the guest copied, put on the pasteboard by us: never reported, also when read again.
    std::fs::write(dir.join("pulled.txt"), b"guest").unwrap();
    poller.apply_set_files(&mut finder, &[dir.join("pulled.txt")]);
    assert_eq!(poller.on_tick_clip(&mut finder), None);
    assert_eq!(poller.report_current(&mut finder), None::<HostClip>);
    std::fs::remove_dir_all(&dir).unwrap();
}

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

/// Each entry's path in the copy, and a file's bytes as its source holds them now.
fn entries(files: Option<HostFiles>) -> Vec<(String, Option<Vec<u8>>)> {
    match files {
        Some(HostFiles::Read(copy)) => copy
            .entries
            .into_iter()
            .map(|entry| (entry.rel, entry.file.map(|file| std::fs::read(file.path).unwrap())))
            .collect(),
        other => panic!("not read: {other:?}"),
    }
}

#[test]
fn a_copy_is_its_items_and_their_tree_without_links_or_finder_icon_metadata() {
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
fn a_name_holding_a_windows_separator_is_repaired_without_path_traversal() {
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
        [
            (".._.._victim.txt".to_owned(), Some(b"evil".to_vec())),
            ("folder".to_owned(), None),
            ("folder\\.._up.txt".to_owned(), Some(b"up".to_vec())),
            ("folder\\ok.txt".to_owned(), Some(b"ok".to_vec())),
        ]
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
    // Trailing dots/spaces are trimmed; no reserved device name is sent.
    for (name, bytes) in [("report.txt", "a"), ("report.txt.", "b"), ("report.txt ", "c"), ("...", "d"), ("nul.txt", "e"), ("Com1", "f"), ("com10", "g")] {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    assert_eq!(
        entries(host_files(&[folder])),
        [
            ("folder".to_owned(), None),
            ("folder\\_".to_owned(), Some(b"d".to_vec())),
            ("folder\\_Com1".to_owned(), Some(b"f".to_vec())),
            ("folder\\_nul.txt".to_owned(), Some(b"e".to_vec())),
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
fn names_meet_as_ntfs_compares_them() {
    let dir = scratch("ntfs-case");
    let mut tops = Vec::new();
    // NTFS compares by each character's simple uppercase: σ and ς are both Σ, but ß is not SS.
    for (sub, name) in [("a", "σ.txt"), ("b", "ς.txt"), ("c", "ß.txt"), ("d", "SS.txt")] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        std::fs::write(dir.join(sub).join(name), sub.as_bytes()).unwrap();
        tops.push(dir.join(sub).join(name));
    }
    let rels: Vec<String> = entries(host_files(&tops)).into_iter().map(|(rel, _)| rel).collect();
    assert_eq!(rels, ["σ.txt", "ς (2).txt", "ß.txt", "SS.txt"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_copy_over_a_limit_is_counted_not_read() {
    let dir = scratch("limits");
    let many = dir.join("many");
    std::fs::create_dir(&many).unwrap();
    for index in 0..CLIP_FILES_BIG_MAX_ENTRIES {
        std::fs::write(many.join(format!("{index}.txt")), b"").unwrap();
    }
    assert_eq!(
        host_files(&[many.clone()]),
        Some(HostFiles::TooLarge { entries: CLIP_FILES_BIG_MAX_ENTRIES + 1, bytes: 0 }),
        "the folder and its 4096 files"
    );
    // Sparse: the walk takes its size, never its bytes.
    let big = dir.join("big.bin");
    std::fs::File::create(&big).unwrap().set_len(CLIP_FILES_BIG_MAX_BYTES + 1).unwrap();
    assert_eq!(
        host_files(&[big]),
        Some(HostFiles::TooLarge { entries: 1, bytes: CLIP_FILES_BIG_MAX_BYTES + 1 })
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_large_copy_is_listed_with_its_sizes_and_times_without_reading_a_byte() {
    let dir = scratch("unread");
    let big = dir.join("big.bin");
    // 8 GiB, sparse: reading it whole would take seconds and as much memory.
    std::fs::File::create(&big).unwrap().set_len(8 << 30).unwrap();
    let Some(HostFiles::Read(copy)) = host_files(std::slice::from_ref(&big)) else {
        panic!("not read");
    };
    assert_eq!(copy.bytes, 8 << 30);
    let source = copy.entries[0].file.as_ref().expect("a file");
    assert_eq!((source.path.as_path(), source.bytes()), (big.as_path(), 8 << 30));
    // The same file is the same copy; a write makes it another one.
    assert_eq!(host_files(std::slice::from_ref(&big)), Some(HostFiles::Read(copy.clone())));
    std::fs::OpenOptions::new().write(true).open(&big).unwrap().set_len(1).unwrap();
    let Some(HostFiles::Read(edited)) = host_files(std::slice::from_ref(&big)) else {
        panic!("not read");
    };
    assert_ne!(edited.digest, copy.digest);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clipboard_folder_never_silently_omits_windows_incompatible_regular_files() {
    let dir = scratch("incompatible-names");
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    for (name, bytes) in [("normal.txt", "normal"), ("NUL.txt", "reserved"), (r"back\slash.txt", "backslash")] {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    let offered = host_files(&[folder]);
    std::fs::remove_dir_all(&dir).unwrap();
    match offered {
        Some(HostFiles::Read(copy)) => {
            let files: Vec<_> = copy.entries.iter().filter(|entry| entry.file.is_some()).collect();
            assert_eq!(files.len(), 3, "clipboard copy silently omitted regular files: expected=3 offered files, seen={files:?}");
            let mut names: Vec<_> = files.iter().map(|entry| entry.rel.as_str()).collect();
            names.sort_unstable();
            assert_eq!(names, ["folder\\_NUL.txt", "folder\\back_slash.txt", "folder\\normal.txt"]);
        }
        Some(HostFiles::Unreadable { .. } | HostFiles::TooLarge { .. }) => {} // Explicit refusal, never a partial copy.
        None => panic!("clipboard copy vanished without an explicit refusal reason"),
    }
}

#[test]
fn clipboard_repaired_components_cannot_steer_a_put_path() {
    let cases = [
        (r"..\..\victim.txt", Some(".._.._victim.txt")),
        (r"a\b", Some("a_b")),
        (r"\", Some("_")),
        ("..", Some("_")),
        (".", Some("_")),
        ("Icon\r", None),
        ("NUL", Some("_NUL")),
        ("nul.txt", Some("_nul.txt")),
        ("COM1.tar.gz", Some("_COM1.tar.gz")),
        ("...", Some("_")),
        ("x\r\n", Some("x__")),
        ("x\u{7f}\u{85}", Some("x__")),
    ];
    for (input, expected) in cases {
        let repaired = windows_name(input);
        assert_eq!(repaired.as_deref(), expected, "input={input:?}");
        if let Some(name) = repaired {
            assert!(!name.contains(['\\', '/']) && !name.chars().any(char::is_control) && name != "." && name != "..",
                "unsafe repaired component: input={input:?} seen={name:?}");
        }
    }
}

#[test]
fn clipboard_repaired_and_unchanged_names_share_sorted_per_folder_numbering() {
    let dir = scratch("repaired-collisions");
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    for name in ["NUL.txt", "_NUL.txt", "_nul.txt.", r"a\b", "a_b", "a_b.", ".DS_Store", "._normal"] {
        std::fs::write(folder.join(name), name.as_bytes()).unwrap();
    }
    let first = entries(host_files(std::slice::from_ref(&folder)));
    assert_eq!(first, entries(host_files(&[folder])));
    let expected = ["folder", "folder\\.DS_Store", "folder\\._normal", "folder\\_NUL.txt", "folder\\_NUL (2).txt",
        "folder\\_nul (3).txt", "folder\\a_b", "folder\\a_b (2)", "folder\\a_b (3)"];
    assert_eq!(first.iter().map(|(rel, _)| rel.as_str()).collect::<Vec<_>>(), expected);
    let keys: std::collections::HashSet<_> = first.iter().map(|(rel, _)| ntfs_key(rel.trim_end_matches(['.', ' ']))).collect();
    assert_eq!(keys.len(), first.len(), "repaired names collided");
    assert_eq!(first.iter().filter(|(_, bytes)| bytes.is_some()).count(), 8);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clipboard_overlong_repairs_and_numbering_are_omitted_and_counted() {
    let dir = scratch("long-names");
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    let device = format!("NUL.{}", "a".repeat(251));
    let plain = "b".repeat(255);
    for name in [&device, &plain] {
        std::fs::write(folder.join(name), b"long").unwrap();
    }
    let offered = entries(host_files(std::slice::from_ref(&folder)));
    assert_eq!(offered.len(), 2, "expected folder and unchanged 255-unit name, seen={offered:?}");
    assert_eq!(offered[1].0, format!("folder\\{plain}"));
    // Two same-named top-level files exercise overflow from the existing numbering.
    let other = dir.join(&plain);
    std::fs::write(&other, b"other").unwrap();
    assert_eq!(entries(host_files(&[folder.join(&plain), other])).len(), 1);
    let mut unsupported = UnsupportedNames::default();
    assert_eq!(unsupported.keep(windows_name(&device).unwrap()), None);
    let mut taken = std::collections::HashSet::new();
    assert_eq!(unsupported.keep(unique_name(plain.clone(), &mut taken)), Some(plain.clone()));
    assert_eq!(unsupported.keep(unique_name(plain, &mut taken)), None);
    assert_eq!(unsupported.0, 2, "expected=2 unsupported components");
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

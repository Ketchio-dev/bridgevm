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

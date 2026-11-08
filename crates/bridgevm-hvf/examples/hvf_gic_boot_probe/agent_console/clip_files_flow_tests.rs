//! Windows Explorer file copies (CF_HDROP) through the resident service loop:
//! CLIPXF polls and GETs against the Rig's Mac and guest (clip_image_tests.rs).

use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A service whose agent announced CF_HDROP, with its startup traffic cleared.
fn rig() -> Rig {
    let mut rig = Rig::unsettled(true);
    rig.guest.html_caps = true;
    rig.guest.files_caps = true;
    rig.settle();
    assert!(rig.h.clip_files);
    assert!(rig.wire.iter().any(|line| line == "CLIPXF"), "polls carry the files");
    rig.verbs();
    take_lines(&mut Vec::new());
    rig
}

/// CLIPXF's files part for `(isDir, top, bytes, path)` entries, as bvagent.ps1 lists them.
fn listing(entries: &[(u8, u8, u64, &str)]) -> String {
    listing_at(entries, 0)
}

/// The same with every file's mtime `mtime`.
fn listing_at(entries: &[(u8, u8, u64, &str)], mtime: u64) -> String {
    let list: String = entries
        .iter()
        .map(|(dir, top, bytes, path)| format!("{dir}|{top}|{bytes}|{}|{path}\n", if *dir == 1 { 0 } else { mtime }))
        .collect();
    format!("{} {}", entries.len(), base64_encode(list.as_bytes()))
}

/// Every entry below `dir`, sorted: its path from `dir` and a file's bytes.
fn tree(dir: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).unwrap().map(Result::unwrap) {
            let path = entry.path();
            let name = path.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                found.push((name, None));
                pending.push(path);
            } else {
                found.push((name, Some(std::fs::read(&path).unwrap())));
            }
        }
    }
    found.sort();
    found
}

/// The GETs on the wire since the last call, by their Windows paths.
fn gets(rig: &mut Rig) -> Vec<String> {
    let gets = rig.wire.iter().filter_map(|line| line.strip_prefix("GET ")).map(Guest::b64_text).collect();
    rig.wire.clear();
    gets
}

#[test]
fn a_guest_file_and_folder_copy_lands_on_the_mac_as_its_top_level_items() {
    let mut rig = rig();
    let (file, folder) = (r"C:\s\보고서 1.txt", r"C:\s\a folder");
    let (inner, sub) = (r"C:\s\a folder\inner.txt", r"C:\s\a folder\sub");
    rig.guest.files.insert(file.into(), vec![1, 2, 3, 4, 5]);
    rig.guest.files.insert(inner.into(), vec![7, 8, 9]);
    rig.guest.listing = Some(listing(&[(0, 1, 5, file), (1, 1, 0, folder), (0, 0, 3, inner), (1, 0, 0, sub)]));
    rig.settle();
    assert_eq!(gets(&mut rig), [file, inner]);
    assert_eq!(rig.mac.file_writes.len(), 1, "one files write");
    let tops = rig.mac.file_writes[0].clone();
    let staging = tops[0].parent().unwrap().parent().unwrap().to_path_buf();
    assert_eq!(staging.parent(), Some(rig.h.clip_files_stage.dir.as_path()));
    assert_eq!(tops, [staging.join("0/보고서 1.txt"), staging.join("1/a folder")]);
    assert_eq!(std::fs::metadata(&staging).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(
        tree(&staging),
        [
            ("0".to_owned(), None),
            ("0/보고서 1.txt".to_owned(), Some(vec![1, 2, 3, 4, 5])),
            ("1".to_owned(), None),
            ("1/a folder".to_owned(), None),
            ("1/a folder/inner.txt".to_owned(), Some(vec![7, 8, 9])),
            ("1/a folder/sub".to_owned(), None),
        ]
    );
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC guest->host files=4 bytes=8 copy=a"]);
    for _ in 0..3 {
        rig.settle();
    }
    assert!(rig.verbs().is_empty(), "neither fetched again nor pushed back");
    assert_eq!(rig.mac.file_writes.len(), 1);
    assert!(rig.mac.writes.is_empty() && rig.mac.rich_writes.is_empty());
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn two_copied_folders_of_one_name_stay_two_items() {
    let mut rig = rig();
    let (a, b) = (r"C:\a\photos", r"C:\b\photos");
    let (a1, b1) = (r"C:\a\photos\one.jpg", r"C:\b\photos\one.jpg");
    rig.guest.files.insert(a1.into(), b"from a".to_vec());
    rig.guest.files.insert(b1.into(), b"from b".to_vec());
    rig.guest.listing = Some(listing(&[(1, 1, 0, a), (0, 0, 6, a1), (1, 1, 0, b), (0, 0, 6, b1)]));
    rig.settle();
    let tops = rig.mac.file_writes.last().expect("published").clone();
    assert_eq!(tops.len(), 2);
    assert_ne!(tops[0], tops[1], "both items are one URL");
    assert_eq!(std::fs::read(tops[0].join("one.jpg")).unwrap(), b"from a");
    assert_eq!(std::fs::read(tops[1].join("one.jpg")).unwrap(), b"from b");
    take_lines(&mut Vec::new());
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn earlier_published_copies_stay_for_a_paste_still_reading_them() {
    let mut rig = rig();
    let file = r"C:\s\a.txt";
    rig.guest.files.insert(file.into(), b"abc".to_vec());
    let mut published = Vec::new();
    for mtime in 1..=CLIP_FILES_KEEP_COPIES as u64 + 1 {
        rig.guest.listing = Some(listing_at(&[(0, 1, 3, file)], mtime));
        rig.settle();
        published.push(rig.mac.file_writes.last().expect("published")[0].clone());
    }
    // Finder may still be pasting from an earlier copy: only the oldest past the bound is gone.
    assert!(!published[0].exists(), "the oldest copy is kept past the bound");
    for path in &published[1..] {
        assert_eq!(std::fs::read(path).unwrap(), b"abc", "{path:?} was deleted while it may be pasting");
    }
    take_lines(&mut Vec::new());
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn a_same_size_edit_copied_again_is_fetched_again() {
    let mut rig = rig();
    let file = r"C:\s\a.txt";
    rig.guest.files.insert(file.into(), b"one".to_vec());
    rig.guest.listing = Some(listing_at(&[(0, 1, 3, file)], 1));
    rig.settle();
    assert_eq!(gets(&mut rig), [file]);
    // Edited in Windows to the same length and copied again: only its mtime tells.
    rig.guest.files.insert(file.into(), b"two".to_vec());
    rig.guest.listing = Some(listing_at(&[(0, 1, 3, file)], 2));
    rig.settle();
    assert_eq!(gets(&mut rig), [file], "the edited file was not fetched again");
    let tops = rig.mac.file_writes.last().unwrap().clone();
    assert_eq!(std::fs::read(&tops[0]).unwrap(), b"two");
    take_lines(&mut Vec::new());
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn a_long_listing_edited_in_the_middle_stages_apart_from_the_published_copy() {
    let mut rig = rig();
    // Over 8 KiB of listing, so the edit is far from both ends of it.
    let files: Vec<String> = (0..160).map(|n| format!(r"C:\s\a fairly long folder name\file-{n:03}.txt")).collect();
    for path in &files {
        rig.guest.files.insert(path.clone(), b"one".to_vec());
    }
    let listed = |middle: u64| {
        let list: String =
            files.iter().enumerate().map(|(n, path)| format!("0|1|3|{}|{path}\n", if n == 80 { middle } else { 1 })).collect();
        format!("{} {}", files.len(), base64_encode(list.as_bytes()))
    };
    rig.guest.listing = Some(listed(1));
    rig.settle();
    let first = rig.mac.file_writes.last().expect("published").clone();
    // The middle file edited in Windows to the same length and the copy taken again.
    rig.guest.files.insert(files[80].clone(), b"two".to_vec());
    rig.guest.listing = Some(listed(2));
    rig.settle();
    let second = rig.mac.file_writes.last().expect("published again").clone();
    assert_ne!(first[0].parent(), second[0].parent(), "the edited copy reused the published copy's directory");
    assert_eq!(std::fs::read(&first[80]).unwrap(), b"one", "the published copy changed under the pasteboard");
    assert_eq!(std::fs::read(&second[80]).unwrap(), b"two");
    take_lines(&mut Vec::new());
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn a_guest_file_copy_over_a_limit_leaves_the_mac_alone() {
    let mut rig = rig();
    let mut names = Vec::new();
    rig.guest.listing = Some(format!("{} -", CLIP_FILES_MAX_ENTRIES + 1));
    rig.settle();
    rig.settle();
    let big = r"C:\s\big.bin";
    rig.guest.files.insert(big.into(), vec![0; 4]);
    rig.guest.listing = Some(listing(&[(0, 1, CLIP_FILES_MAX_BYTES + 1, big)]));
    rig.settle();
    rig.settle();
    assert_eq!(
        take_lines(&mut names),
        [
            "BVAGENT CLIPSYNC refused guest-files too-large files=257 bytes=0 copy=a".to_owned(),
            format!(
                "BVAGENT CLIPSYNC refused guest-files too-large files=1 bytes={} copy=b",
                CLIP_FILES_MAX_BYTES + 1
            ),
        ]
    );
    assert!(rig.verbs().is_empty(), "nothing fetched");
    assert!(rig.mac.file_writes.is_empty() && rig.mac.writes.is_empty());
    assert!(!rig.h.clip_files_stage.dir.exists(), "nothing staged");
}

#[test]
fn a_guest_path_that_could_leave_its_copied_folder_is_refused_whole() {
    let mut rig = rig();
    let evil = rig.h.clip_files_stage.dir.with_file_name("evil.txt");
    let cases: [&[(u8, u8, u64, &str)]; 6] = [
        &[(1, 1, 0, r"C:\s\f"), (0, 0, 4, r"C:\s\f\..\..\..\evil.txt")],
        &[(1, 1, 0, r"C:\s\f"), (0, 0, 4, r"C:\s\other\evil.txt")],
        &[(1, 1, 0, r"C:\s\f"), (0, 0, 4, r"C:\s\fx\evil.txt")],
        &[(0, 1, 4, r"C:\s\a.txt"), (0, 0, 4, r"C:\s\a.txt\evil.txt")],
        &[(1, 1, 0, r"C:\s\f"), (0, 0, 4, r"C:\s\f\a/../../evil.txt")],
        &[(0, 1, 4, r"C:\s\..")],
    ];
    for entries in cases {
        for (_, _, _, path) in entries {
            rig.guest.files.insert(path.to_string(), b"evil".to_vec());
        }
        rig.guest.listing = Some(listing(entries));
        rig.settle();
    }
    let lines = take_lines(&mut Vec::new());
    assert_eq!(lines.len(), cases.len(), "{lines:#?}");
    for (index, line) in lines.iter().enumerate() {
        let copy = char::from(b'a' + index as u8);
        let refused = line.starts_with("BVAGENT CLIPSYNC refused guest-files unsafe-path files=");
        assert!(refused && line.ends_with(&format!(" copy={copy}")), "{line}");
    }
    assert!(gets(&mut rig).is_empty(), "nothing fetched");
    assert!(rig.mac.file_writes.is_empty());
    assert!(!rig.h.clip_files_stage.dir.exists(), "nothing staged");
    assert!(!evil.exists(), "nothing written outside the staging directory");
}

#[test]
fn a_newer_guest_copy_mid_transfer_abandons_the_older_one() {
    let mut rig = rig();
    let folder = r"C:\s\A";
    let older: Vec<String> = (1..=3).map(|n| format!(r"{folder}\{n}.txt")).collect();
    for path in &older {
        rig.guest.files.insert(path.clone(), b"aa".to_vec());
    }
    let newer = r"C:\s\b.txt";
    rig.guest.files.insert(newer.into(), b"bb".to_vec());
    rig.guest.listing = Some(listing(&[
        (1, 1, 0, folder),
        (0, 0, 2, &older[0]),
        (0, 0, 2, &older[1]),
        (0, 0, 2, &older[2]),
    ]));
    // Windows copies b.txt while the first file comes over.
    rig.guest.then_listing = Some(listing(&[(0, 1, 2, newer)]));
    rig.settle();
    rig.settle();
    let fetched = gets(&mut rig);
    assert_eq!(fetched, [older[0].as_str(), older[1].as_str(), newer], "the older copy's last file is never fetched");
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC guest->host files=1 bytes=2 copy=a"]);
    let copy = rig.h.last_synced.guest_copy;
    let staging = rig.h.clip_files_stage.dir.join(copy.to_string());
    assert_eq!(rig.mac.file_writes, [vec![staging.join("0/b.txt")]]);
    assert_eq!(
        tree(&rig.h.clip_files_stage.dir),
        [(copy.to_string(), None), (format!("{copy}/0"), None), (format!("{copy}/0/b.txt"), Some(b"bb".to_vec()))],
        "the older copy's staging directory is gone"
    );
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn a_malformed_file_list_counts_as_no_files() {
    let mut rig = rig();
    // A list one entry short of its count, a count that is no number, and no base64.
    let one = base64_encode(format!("0|1|2|0|{}\n", r"C:\s\a.txt").as_bytes());
    for part in [format!("2 {one}"), "x -".to_owned(), "1 not-base64".to_owned()] {
        rig.guest.text = format!("t {part}");
        rig.guest.listing = Some(part);
        rig.settle();
        rig.settle();
    }
    let lines = take_lines(&mut Vec::new());
    assert_eq!(lines.len(), 6, "{lines:#?}");
    for (pair, copy) in lines.chunks(2).zip(['a', 'b', 'c']) {
        assert_eq!(pair[0], format!("BVAGENT CLIPSYNC refused guest-files malformed copy={copy}"));
        let synced = pair[1].starts_with("BVAGENT CLIPSYNC guest->host bytes=");
        assert!(synced && pair[1].ends_with(&format!(" copy={copy}")), "{}", pair[1]);
    }
    assert!(rig.mac.file_writes.is_empty());
    assert_eq!(rig.mac.text.as_deref(), Some("t 1 not-base64"), "the text still syncs");
}

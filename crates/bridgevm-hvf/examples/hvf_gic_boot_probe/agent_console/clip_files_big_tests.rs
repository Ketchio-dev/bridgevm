//! Clipboard file copies past the old limits (64 MiB, 256 entries), with an agent whose READY
//! carries `bigfile`: Windows files come in GETRANGE ranges written as they arrive, Mac files go
//! in LPUT chunks read as they are sent, every step its own request, against the Rig's Mac and
//! guest (clip_image_tests.rs).

use super::*;
use std::path::Path;

const MIB: u64 = 1 << 20;

/// A service whose agent has `bigfile` and announced both file verbs, its startup traffic cleared.
fn rig() -> Rig {
    let mut rig = Rig::unsettled(true);
    rig.guest.html_caps = true;
    rig.guest.files_caps = true;
    rig.guest.set_files_caps = true;
    rig.h.share_big = true;
    rig.h.put_data = true;
    rig.settle();
    assert!(rig.h.clip_files && rig.h.clip_files_push);
    rig.verbs();
    take_lines(&mut Vec::new());
    rig
}

/// CLIPXF's files part for `(isDir, top, bytes, path)` entries, as bvagent.ps1 lists them.
fn listing(entries: &[(u8, u8, u64, &str)]) -> String {
    let list: String = entries
        .iter()
        .map(|(dir, top, bytes, path)| format!("{dir}|{top}|{bytes}|{}|{path}\n", 1 - dir))
        .collect();
    format!("{} {}", entries.len(), base64_encode(list.as_bytes()))
}

/// One request out and every reply to it in; the request's first line (its header), if any went.
fn step(rig: &mut Rig) -> Option<String> {
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |bytes| sent.push(bytes.to_vec()));
    let first = sent.first().map(|bytes| String::from_utf8_lossy(bytes).split('\n').next().unwrap().to_owned());
    let mut wire: VecDeque<Vec<u8>> = sent.into();
    while let Some(bytes) = wire.pop_front() {
        for reply in rig.guest.answer(&String::from_utf8(bytes).unwrap()) {
            rig.h.handle_service_reply_with_sender(&reply, rig.now, &mut |bytes| wire.push_back(bytes.to_vec()));
            // Never a whole range or file in memory: nothing accumulates as a GET would.
            assert!(rig.h.get_accum.as_ref().is_none_or(|get| get.bytes.capacity() == 0));
        }
    }
    first
}

/// A Mac file of `len` bytes, each 1 MiB block its own; its SHA-256.
fn mac_file(path: &Path, len: u64) -> [u8; 32] {
    let mut file = std::fs::File::create(path).unwrap();
    let mut sha = Sha256::new();
    let mut block = vec![0u8; MIB as usize];
    for index in 0..len.div_ceil(MIB) {
        for (at, byte) in block.iter_mut().enumerate() {
            *byte = (at as u8).wrapping_mul(7) ^ (index as u8).wrapping_mul(13) ^ (at >> 9) as u8;
        }
        let part = &block[..(len - index * MIB).min(MIB) as usize];
        sha.update(part);
        std::io::Write::write_all(&mut file, part).unwrap();
    }
    sha.finalize().into()
}

/// A fresh Mac folder for one test's copied items.
fn mac_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bridgevm-files-big-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Every file below `dir`, by its path from it.
fn staged_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().map(Result::unwrap) {
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                found.push(entry.path().strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_200_mib_windows_file_comes_over_in_ranges_written_as_they_arrive() {
    let mut rig = rig();
    let big = r"C:\s\video.mp4";
    let len = 200 * MIB;
    rig.guest.big.insert(big.into(), len);
    rig.guest.listing = Some(listing(&[(0, 1, len, big)]));
    // The next poll lists it.
    rig.now += Duration::from_secs(1);
    rig.h.service_enqueue(rig.now);
    let mut ranges = Vec::new();
    while let Some(line) = step(&mut rig) {
        if let Some(rest) = line.strip_prefix("GETRANGE ") {
            let fields: Vec<&str> = rest.split(' ').collect();
            assert_eq!(Guest::b64_text(fields[0]), big);
            ranges.push((fields[1].parse::<u64>().unwrap(), fields[2].parse::<u64>().unwrap()));
            // A clipboard poll and a control command asked for mid-file go before the next range.
            if ranges.len() == 3 {
                rig.h.queue.push_back(ServiceReq::ClipPoll);
                rig.h.queue.push_back(ServiceReq::Ctl("whoami".into()));
                assert_eq!(step(&mut rig).as_deref(), Some("CLIPXF"));
                assert!(step(&mut rig).is_some_and(|line| line.starts_with("RUN ")));
                rig.h.complete_in_flight();
            }
        }
    }
    assert!(!ranges.is_empty() && ranges.iter().all(|(_, len)| *len <= SHARE_RANGE_BYTES), "{ranges:?}");
    let offsets: Vec<u64> = ranges.iter().map(|(offset, _)| *offset).collect();
    let expected: Vec<u64> = (0..len.div_ceil(SHARE_RANGE_BYTES)).map(|n| n * SHARE_RANGE_BYTES).collect();
    assert_eq!(offsets, expected, "each range once, in order");
    assert!(rig.worker.apply_pending(&mut rig.mac));
    let top = rig.mac.file_writes.last().expect("published")[0].clone();
    let file = std::fs::File::open(&top).unwrap();
    assert_eq!(file.metadata().unwrap().len(), len);
    let mut block = vec![0u8; MIB as usize];
    for index in 0..len / MIB {
        std::os::unix::fs::FileExt::read_exact_at(&file, &mut block, index * MIB).unwrap();
        let wanted: Vec<u8> = (index * MIB..(index + 1) * MIB).map(ranged_byte).collect();
        assert!(block == wanted, "MiB {index} differs");
    }
    assert_eq!(
        take_lines(&mut Vec::new()),
        [format!("BVAGENT CLIPSYNC guest->host files=1 bytes={len} copy=a")]
    );
    std::fs::remove_dir_all(&rig.h.clip_files_stage.dir).unwrap();
}

#[test]
fn a_200_mib_mac_file_goes_in_chunks_read_as_they_are_sent() {
    let mut rig = rig();
    let dir = mac_dir("200");
    let path = dir.join("disk.img");
    let len = 200 * MIB;
    let sha = mac_file(&path, len);
    rig.mac.copy_files("disk.img", vec![path]);
    assert!(rig.worker.poll(&mut rig.mac));
    rig.h.service_enqueue(rig.now);
    let (mut verbs, mut chunks) = (Vec::new(), 0);
    while let Some(line) = step(&mut rig) {
        let verb = line.split(' ').next().unwrap().to_owned();
        if verb == "LPUTDATA" {
            chunks += 1;
            let chars: usize = line.rsplit(' ').next().unwrap().parse().unwrap();
            assert!(chars <= SHARE_PUT_DATA_CHUNK_BYTES.div_ceil(3) * 4, "a chunk past 1 MiB");
            // One chunk at a time: the next step is all that waits, and the line buffer holds one.
            let steps = rig.h.queue.iter().filter(|req| matches!(req, ServiceReq::ClipFilesPush(_))).count();
            assert_eq!(steps, 1);
            assert!(rig.h.service_line_scratch.capacity() < 4 * SHARE_PUT_DATA_CHUNK_BYTES);
            if chunks == 5 {
                rig.h.queue.push_back(ServiceReq::ClipPoll);
                assert_eq!(step(&mut rig).as_deref(), Some("CLIPXF"), "the poll waited for the file");
            }
        }
        if verbs.last() != Some(&verb) {
            verbs.push(verb);
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(verbs, ["LPUTBEG", "LPUTDATA", "LPUTEND", "CLIPSETFILES"]);
    assert_eq!(chunks, 200);
    let folder = &rig.guest.set_files[0].0;
    let guest = format!(r"{DIR}\files\{folder}\disk.img");
    assert_eq!(rig.guest.lput_files.get(&guest), Some(&(len, sha)), "the guest's file is not the Mac's");
    assert_eq!(rig.guest.lput_dropped, 0);
    assert_eq!(
        take_lines(&mut Vec::new()),
        [format!("BVAGENT CLIPSYNC host->guest files=1 bytes={len} copy=a")]
    );
}

#[test]
fn a_newer_copy_mid_range_leaves_no_part_of_the_older_one() {
    // A newer Windows copy, then a newer Mac copy, each while a large file is on its way.
    for mac in [false, true] {
        let mut rig = rig();
        let big = r"C:\s\big.bin";
        // Past the old limit, so it comes in ranges.
        let len = CLIP_FILES_MAX_BYTES + 1;
        rig.guest.big.insert(big.into(), len);
        rig.guest.listing = Some(listing(&[(0, 1, len, big)]));
        let newer = r"C:\s\small.txt";
        rig.guest.files.insert(newer.into(), b"small".to_vec());
        if mac {
            rig.mac_copy_mid_get = true;
        } else {
            rig.guest.then_listing = Some(listing(&[(0, 1, 5, newer)]));
        }
        rig.settle();
        rig.settle();
        let ranges = rig.wire.iter().filter(|line| line.starts_with("GETRANGE ")).count();
        assert_eq!(ranges, 1, "the older copy was fetched on (mac {mac})");
        let staged = staged_files(&rig.h.clip_files_stage.dir);
        assert!(staged.iter().all(|file| !file.ends_with("big.bin")), "part of the older copy stayed: {staged:?}");
        if mac {
            assert!(rig.mac.file_writes.is_empty(), "the Windows copy replaced a newer Mac copy");
        } else {
            assert_eq!(staged.len(), 1, "{staged:?}");
            let top = rig.mac.file_writes.last().expect("the newer copy published")[0].clone();
            assert_eq!(std::fs::read(top).unwrap(), b"small");
        }
        take_lines(&mut Vec::new());
        let _ = std::fs::remove_dir_all(&rig.h.clip_files_stage.dir);
    }
}

/// The Mac copy goes out: its first LPUTBEG and two chunks are in.
fn start_large_push(rig: &mut Rig, dir: &Path) {
    let path = dir.join("big.bin");
    mac_file(&path, CLIP_FILES_MAX_BYTES + 3 * MIB);
    rig.mac.copy_files("big.bin", vec![path]);
    assert!(rig.worker.poll(&mut rig.mac));
    rig.h.service_enqueue(rig.now);
    assert!(step(rig).is_some_and(|line| line.starts_with("LPUTBEG ")));
    for _ in 0..2 {
        assert!(step(rig).is_some_and(|line| line.starts_with("LPUTDATA ")));
    }
    assert!(rig.guest.lput.is_some());
}

#[test]
fn a_newer_mac_copy_mid_file_deletes_the_guest_temporary_and_goes_alone() {
    let mut rig = rig();
    let dir = mac_dir("newer-mac");
    start_large_push(&mut rig, &dir);
    rig.mac.copy(Some("newer"), None);
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.verbs(), verbs(&["LPUTABORT", "CLIPSET"]));
    assert!(rig.guest.lput.is_none() && rig.guest.lput_files.is_empty(), "a part of the file stayed");
    assert_eq!(rig.guest.lput_dropped, 1);
    assert!(rig.guest.set_files.is_empty());
    assert_eq!(rig.guest.text, "newer");
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC host->guest bytes=5 copy=a"]);
}

#[test]
fn a_windows_copy_polled_mid_push_wins_unless_it_may_predate_the_mac_copy() {
    let mut rig = rig();
    let dir = mac_dir("newer-guest");
    start_large_push(&mut rig, &dir);
    // Polled within the grace: may predate the Mac copy, which wins as for any push.
    rig.guest.text = "earlier".into();
    rig.h.queue.push_front(ServiceReq::ClipPoll);
    assert_eq!(step(&mut rig).as_deref(), Some("CLIPXF"));
    assert!(rig.mac.writes.is_empty() && rig.guest.lput.is_some(), "the push ended for an older copy");
    // Copied in Windows well after the Mac copy: the push ends, its temporary goes.
    rig.now += CLIP_FILES_PUSH_GRACE;
    rig.guest.text = "later".into();
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.verbs(), verbs(&["LPUTABORT"]));
    assert!(rig.guest.lput.is_none() && rig.guest.set_files.is_empty());
    assert_eq!(rig.mac.text.as_deref(), Some("later"), "the newer Windows copy did not reach the Mac");
    assert_eq!(take_lines(&mut Vec::new()), ["BVAGENT CLIPSYNC guest->host bytes=5 copy=a"]);
}

#[test]
fn a_shared_folder_large_put_and_a_push_file_never_share_the_agent_large_put() {
    let mut rig = rig();
    let dir = mac_dir("share-turns");
    let share_dir = dir.join("share");
    std::fs::create_dir(&share_dir).unwrap();
    let mut share = ShareState::new(ShareSync::new(1 << 30), share_dir.clone(), r"C:\share".into(), Duration::from_secs(3600));
    share.last_poll = Some(rig.now);
    share.engine.end_first_listing_wait();
    rig.h.share = Some(share);
    start_large_push(&mut rig, &dir);
    // A large shared-folder file found mid-push waits for the push's file.
    std::fs::write(share_dir.join("movie.mov"), vec![1u8; SHARE_RANGE_BYTES as usize + 1]).unwrap();
    assert!(rig.h.enqueue_one_share_host_change(rig.now));
    let mut order = Vec::new();
    while let Some(line) = step(&mut rig) {
        if line.starts_with("LPUTBEG ") {
            order.push(Guest::b64_text(line.split(' ').nth(1).unwrap()));
        }
        if line.starts_with("LPUTEND ") || line.starts_with("LPUTABORT") {
            order.push(line.clone());
        }
        rig.h.advance_big_share(rig.now);
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(order.len(), 3, "{order:?}");
    assert!(order[0].starts_with("LPUTEND "), "the share's file began inside the push's: {order:?}");
    assert_eq!(order[1], r"C:\share\movie.mov");
    assert!(order[2].starts_with("LPUTEND "));
    assert_eq!(rig.guest.lput_dropped, 0, "a temporary was dropped");
    assert_eq!(rig.guest.set_files.len(), 1);
    assert_eq!(rig.guest.lput_files.len(), 2);
}

#[test]
fn a_large_copy_needs_room_on_the_receiving_side() {
    // Windows to the Mac: the copy plus the shared folder's margin must fit.
    let mut rig = rig();
    rig.h.clip_files_stage.free_margin = u64::MAX / 2;
    let big = r"C:\s\big.bin";
    rig.guest.big.insert(big.into(), CLIP_FILES_MAX_BYTES + 1);
    rig.guest.listing = Some(listing(&[(0, 1, CLIP_FILES_MAX_BYTES + 1, big)]));
    rig.settle();
    assert!(rig.verbs().is_empty(), "fetched without the room for it");
    assert!(staged_files(&rig.h.clip_files_stage.dir).is_empty());
    // The Mac to Windows: the agent refuses a file its disk has no room for.
    rig.guest.disk_full = true;
    let dir = mac_dir("full");
    let path = dir.join("big.bin");
    std::fs::File::create(&path).unwrap().set_len(CLIP_FILES_MAX_BYTES + 1).unwrap();
    rig.mac.copy_files("big.bin", vec![path]);
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.verbs(), verbs(&["LPUTBEG"]));
    let bytes = CLIP_FILES_MAX_BYTES + 1;
    assert_eq!(
        take_lines(&mut Vec::new()),
        [
            format!("BVAGENT CLIPSYNC refused guest-files low-space files=1 bytes={bytes} copy=a"),
            format!("BVAGENT CLIPSYNC skip host-files guest-full files=1 bytes={bytes} copy=b"),
        ]
    );
    let _ = std::fs::remove_dir_all(&rig.h.clip_files_stage.dir);
}

#[test]
fn an_agent_without_bigfile_keeps_whole_file_gets_and_puts_and_their_limits() {
    let mut rig = rig();
    rig.h.share_big = false;
    rig.h.put_data = false;
    // Within the old limits: one GET, as before.
    let small = r"C:\s\a.txt";
    rig.guest.files.insert(small.into(), b"abc".to_vec());
    rig.guest.listing = Some(listing(&[(0, 1, 3, small)]));
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["GET"]));
    // Over them: refused with the limits it was over.
    let big = r"C:\s\big.bin";
    rig.guest.big.insert(big.into(), CLIP_FILES_MAX_BYTES + 1);
    rig.guest.listing = Some(listing(&[(0, 1, CLIP_FILES_MAX_BYTES + 1, big)]));
    rig.settle();
    assert!(rig.verbs().is_empty());
    // A Mac copy over them goes as its names; one within them as whole-file PUTs.
    let dir = mac_dir("old");
    let path = dir.join("big.bin");
    std::fs::File::create(&path).unwrap().set_len(CLIP_FILES_MAX_BYTES + 1).unwrap();
    rig.mac.copy_files("big.bin", vec![path]);
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPSET"]));
    std::fs::write(dir.join("a.txt"), b"abc").unwrap();
    rig.mac.copy_files("a.txt", vec![dir.join("a.txt")]);
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(rig.verbs(), verbs(&["PUTBEG", "PUTCHUNK", "PUTEND", "CLIPSETFILES"]));
    let lines = take_lines(&mut Vec::new());
    let limits = "limit-files=256 limit-bytes=67108864";
    let bytes = CLIP_FILES_MAX_BYTES + 1;
    assert_eq!(
        lines,
        [
            "BVAGENT CLIPSYNC guest->host files=1 bytes=3 copy=a".to_owned(),
            format!("BVAGENT CLIPSYNC refused guest-files too-large files=1 bytes={bytes} {limits} copy=b"),
            format!("BVAGENT CLIPSYNC skip host-files too-large files=1 bytes={bytes} {limits} copy=c"),
            "BVAGENT CLIPSYNC host->guest bytes=7 copy=c".to_owned(),
            "BVAGENT CLIPSYNC host->guest files=1 bytes=3 copy=d".to_owned(),
        ]
    );
    let _ = std::fs::remove_dir_all(&rig.h.clip_files_stage.dir);
}

#[test]
fn an_agent_restart_mid_push_starts_the_large_push_over() {
    let mut rig = rig();
    let dir = mac_dir("restart");
    start_large_push(&mut rig, &dir);
    rig.h.handle_service_reply_with_sender("READY GUEST v3-share2 putdata bigfile", rig.now, &mut |_| {});
    // The new agent holds no temporary.
    rig.guest.lput = None;
    rig.settle();
    std::fs::remove_dir_all(&dir).unwrap();
    let verbs: Vec<String> = rig.verbs().into_iter().filter(|verb| verb != "LPUTDATA").collect();
    assert_eq!(verbs, ["CLIPCAPS", "LPUTBEG", "LPUTEND", "CLIPSETFILES"]);
    assert_eq!(rig.guest.lput_files.len(), 1);
}

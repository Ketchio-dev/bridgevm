//! A guest folder of 100,000 files in a code repository's tree (paths of about 60 characters):
//! its listing is applied, not refused, a new session adopts the copies both sides hold, and
//! LSSAME applies the kept listing again. The test prints how long each step took; for the
//! release numbers:
//! `cargo test --release --example hvf_gic_boot_probe 100k -- --nocapture`.

use super::*;

const FILES: usize = 100_000;
const MS: u128 = 1_794_096_312_123;

fn names() -> Vec<String> {
    (0..FILES)
        .map(|i| format!("repo/src/node_modules/pkg-{:03}/lib/part-{:02}/file-{i:06}.js", i % 400, i % 25))
        .collect()
}

/// The agent's listing of `names` (all 100 bytes, written at MS) with their folders.
fn listing(names: &[String]) -> String {
    let time = dotnet_time(MS);
    let mut folders = std::collections::BTreeSet::new();
    let mut listing = String::new();
    for name in names {
        let name = name.replace('/', "\\");
        for (i, _) in name.match_indices('\\') {
            folders.insert(name[..i].to_string());
        }
        listing.push_str(&format!("{name}|100|0|{time}\r\n"));
    }
    for folder in folders {
        listing.push_str(&format!("{folder}|0|1|{time}\r\n"));
    }
    listing
}

#[test]
fn a_100k_entry_listing_is_applied_and_a_new_session_adopts_its_copies() {
    let names = names();
    let listing = listing(&names);
    assert!(listing.lines().count() > FILES && listing.len() > 7_000_000, "{} bytes", listing.len());
    let line = format!("LSOK {} g1", base64_encode(listing.as_bytes()));

    // A new session: the Mac holds every file and waits for the first listing.
    let mut engine = ShareSync::new(512);
    let clock = Instant::now();
    let host = names.iter().map(|name| HostFile { name: name.clone(), size: 100, mtime_ms: MS });
    engine.on_host_scan_normalized(host);
    let scanned = clock.elapsed();
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.share = Some(ShareState::new(engine, PathBuf::from("/unused-test-root"), ROOT.into(), Duration::from_secs(2)));
    h.note_agent_ready("GUEST v3-share2 lssame", Instant::now());
    let _ = ShareState::take_pause_lines();
    let reply = |h: &mut AgentConsoleHarness, line: &str| {
        let now = Instant::now();
        h.in_flight = Some((ServiceReq::ShareLs, now));
        h.handle_service_reply(line, None, None, now);
        assert!(h.in_flight.is_none());
        now.elapsed()
    };

    let applied = reply(&mut h, &line);
    assert!(ShareState::take_pause_lines().is_empty(), "a 100k-entry listing paused the share");
    assert!(h.queue.is_empty(), "the waiting Mac files hold the listed copies: {} requests", h.queue.len());

    let engine = &mut h.share.as_mut().unwrap().engine;
    let clock = Instant::now();
    let adopted = names.iter().filter(|name| engine.adopt_listed_version(name, 100, MS, || None)).count();
    assert_eq!(adopted, FILES);
    let adopting = clock.elapsed();

    let same = reply(&mut h, "LSSAME g1");
    assert!(h.queue.is_empty(), "an unchanged listing planned {} requests", h.queue.len());
    let mut line = String::new();
    h.share.as_ref().unwrap().listing_same.write_lsr_gen(&mut line);
    assert_eq!(line, " g1", "the listing is no longer kept");
    println!(
        "{FILES} files ({} listing bytes): host scan {scanned:?}, listing applied {applied:?}, \
         adopted {adopting:?}, LSSAME applied {same:?}",
        listing.len()
    );
}

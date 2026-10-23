//! Guest listing names refused by the host name limits are reported, not
//! dropped silently, and never printed: a fixed reason and the name's byte
//! length for the first few, one line counting them all, and nothing again
//! while the next listings refuse the same names.

use super::*;
use crate::agent_console::share::share_listing_report::refused_name_lines;

/// A guest-form name of `bytes` bytes: ten 99-byte folders, then a file name.
fn long_path(bytes: usize) -> String {
    format!("{}\\", "d".repeat(99)).repeat(10) + &"f".repeat(bytes - 1000)
}

fn listing(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("{name}|1|0|m1\n"))
        .collect()
}

/// Parse `listing` as the resident service does; return its entries and report lines.
fn report(engine: &mut ShareSync, listing: &str) -> (Vec<String>, Vec<String>) {
    let mut entries = Vec::new();
    let refused = share_sync::parse_ls_into(listing, &mut entries);
    let names = entries.into_iter().map(|entry| entry.name).collect();
    (names, refused_name_lines(engine, &refused))
}

#[test]
fn refused_names_are_reported_by_reason_and_byte_length_only() {
    let mut engine = ShareSync::new(1024);
    // 256 Hangul syllables are 768 bytes, under PATH_MAX, but one over NAME_MAX.
    let component = format!("sub\\{}", "가".repeat(256));
    let names = [
        long_path(1024),
        component,
        long_path(5000),
        "..\\up.txt".into(),
        long_path(2000),
        "ok.txt".into(),
    ];
    let (synced, lines) = report(&mut engine, &listing(&names));
    assert_eq!(synced, ["ok.txt"]);
    assert_eq!(
        lines,
        [
            "BVAGENT SHARE skip <guest-name> path-too-long 1024",
            "BVAGENT SHARE skip <guest-name> component-too-long 772",
            "BVAGENT SHARE skip <guest-name> path-too-long 5000",
            // The `..` name is refused as before, but not as a host-limit name.
            "BVAGENT SHARE skip <guest-names> over-host-limits 4",
        ]
    );
}

#[test]
fn a_hostile_listing_gets_a_few_lines_and_one_count_line() {
    let mut engine = ShareSync::new(1024);
    let names: Vec<String> = (0..10_000).map(|i| long_path(1024 + i % 50)).collect();
    let (synced, lines) = report(&mut engine, &listing(&names));
    assert!(synced.is_empty());
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(
        lines[3],
        "BVAGENT SHARE skip <guest-names> over-host-limits 10000"
    );
    assert!(
        lines
            .iter()
            .all(|line| line.len() < 64 && !line.contains("ddd")),
        "{lines:?}"
    );
}

#[test]
fn the_same_refused_names_are_reported_once_until_they_change() {
    let mut engine = ShareSync::new(1024);
    let refused = listing(&[long_path(1500), "ok.txt".into()]);
    assert_eq!(report(&mut engine, &refused).1.len(), 2);
    assert!(
        report(&mut engine, &refused).1.is_empty(),
        "an unchanged listing was reported again"
    );
    let changed = listing(&[long_path(1500), long_path(1600), "ok.txt".into()]);
    assert_eq!(report(&mut engine, &changed).1.len(), 3);
    assert!(report(&mut engine, &listing(&["ok.txt".into()]))
        .1
        .is_empty());
    let (synced, lines) = report(&mut engine, &refused);
    assert_eq!(
        (synced, lines.len()),
        (vec!["ok.txt".to_string()], 2),
        "a refused name that came back was not reported"
    );
}

//! The too-large line the app's notice reads (HvfShareTooLargeNoticeTests uses the same text).
#![cfg(test)]

use super::*;

#[test]
fn a_too_large_line_names_the_limit_that_applied_and_the_side() {
    assert_eq!(
        too_large_line("휴가 영상.mov", 200_000_000, 64 * 1024 * 1024, "mac"),
        "BVAGENT SHARE skip 휴가 영상.mov too-large 200000000 limit=67108864 mac"
    );
    let mut sync = ShareSync::new(16 * 1024 * 1024);
    sync.set_whole_file_limit(Some(64 * 1024 * 1024));
    assert_eq!(sync.max_bytes(), 64 * 1024 * 1024, "the limit before the agent moves files in pieces");
}

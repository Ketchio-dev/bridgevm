//! Report bounds remain independent of the whole-listing admission bounds.

use super::*;

#[test]
fn ten_thousand_refused_names_get_a_few_lines_and_one_count_line() {
    let mut engine = ShareSync::new(1024);
    let mut refused = share_sync::RefusedNames::default();
    for bytes in 1024..1027 {
        assert!(refused.key(&long_path(bytes)).is_none());
    }
    // Model the count directly: over-limit listings no longer reach reporting.
    refused.count = 10_000;
    let lines = refused_name_lines(&mut engine, &refused);
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

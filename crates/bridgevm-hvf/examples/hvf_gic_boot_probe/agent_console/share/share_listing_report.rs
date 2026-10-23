//! Skip report for guest listing names the host name limits refused.

use super::*;

impl ShareState {
    /// Parse a guest listing into the scratch entries and print the skip
    /// report for the names the host limits refused.
    pub(in crate::agent_console) fn parse_guest_listing(&mut self, listing: &str) {
        let refused = share_sync::parse_ls_into(listing, &mut self.guest_ls_scratch);
        for line in refused_name_lines(&mut self.engine, &refused) {
            println!("{line}");
        }
    }
}

/// At most `REFUSED_SHOWN` lines, each a fixed reason and a byte length but
/// never the name, then one count line, so a hostile listing cannot flood
/// run.log. No lines when nothing was refused or the same names were refused
/// by the previous listing, so a listing polled again is reported once.
pub(in crate::agent_console) fn refused_name_lines(
    engine: &mut ShareSync,
    refused: &share_sync::RefusedNames,
) -> Vec<String> {
    if !engine.refused_names_changed(refused) || refused.count == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = refused
        .shown
        .iter()
        .map(|(reason, bytes)| format!("BVAGENT SHARE skip <guest-name> {reason} {bytes}"))
        .collect();
    lines.push(format!(
        "BVAGENT SHARE skip <guest-names> over-host-limits {}",
        refused.count
    ));
    lines
}

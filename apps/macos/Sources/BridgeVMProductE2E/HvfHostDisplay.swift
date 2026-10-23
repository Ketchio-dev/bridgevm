import Foundation

/// The ramfb display exporter's record in the post-footer host tail, as
/// scripts/live-gates/hvf_host_display.py states it. The platform prints a running
/// exporter's `ramfb display timing:` record once as it drops, after the HDA
/// controller's records and the CoreAudio teardown records, so it appears at most
/// once and after every HDA diagnostic, stats or continuity record. Fields are
/// unsigned decimals without leading zeros. A tail without it reads as before.
extension HvfHostTail {
    static let displayRecord: (prefix: String, fields: [String]) = ("ramfb display timing:", [
        "ticks", "late_le_1ms", "late_le_4ms", "late_le_8ms", "late_le_16ms", "late_gt_16ms", "max_late_us", "published",
        "interval_le_20ms", "interval_le_40ms", "interval_le_70ms", "interval_gt_70ms", "max_interval_ms",
    ])
    static let displayLine = "^" + displayRecord.prefix + displayRecord.fields.map { " \($0)=\(diagnosticValue)" }.joined() + "$"

    /// At most one display record, and no HDA diagnostic, stats or continuity record after it.
    static func displayPlaced<S: Sequence>(_ records: S) -> Bool where S.Element: StringProtocol {
        let hda = ["hda CoreAudio stats:", "hda CoreAudio continuity:"] + diagnosticRecords.map(\.prefix)
        var shown = false
        for record in records {
            if record.hasPrefix(displayRecord.prefix) {
                guard !shown else { return false }
                shown = true
            } else if shown, hda.contains(where: { record.hasPrefix($0) }) {
                return false
            }
        }
        return true
    }
}

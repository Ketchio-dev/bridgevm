import Foundation

/// The HDA diagnostic records the host prints in the post-footer tail, as
/// scripts/live-gates/hvf_host_diagnostics.py states them. The HDA controller
/// prints `hda poll:`, `hda stream:`, `hda repeat:` and its CoreAudio sink's
/// `hda CoreAudio timing:` as the platform drops it, just before the sink's
/// lifecycle, stats and continuity records, so each appears at most once, in
/// that order and before any stats or continuity record. Fields are unsigned
/// decimals without leading zeros, `*_ms` buffer lengths carry three decimals
/// and position_buffer is 0 or 1.
extension HvfHostTail {
    static let diagnosticRecords: [(prefix: String, fields: [String])] = [
        ("hda poll:", ["polls", "gap_le_10ms", "gap_le_20ms", "gap_le_50ms", "gap_le_100ms", "gap_gt_100ms",
                       "max_gap_ms", "capped_polls", "capped_lost_ms", "ioc_completions", "max_iocs_per_poll",
                       "msis_sent"]),
        ("hda stream:", ["runs", "rate_hz", "channels", "bits", "cbl_bytes", "cbl_ms", "entries", "entry_bytes_min",
                         "entry_bytes_max", "entry_ms_min", "entry_ms_max", "ioc_entries", "position_buffer",
                         "lpib_reads", "running_ms"]),
        ("hda repeat:", ["passes", "silent_passes", "compared_passes", "repeat_passes", "max_repeat_run"]),
        ("hda CoreAudio timing:", ["callbacks", "interval_le_10ms", "interval_le_15ms", "interval_le_20ms",
                                   "interval_le_30ms", "interval_le_50ms", "interval_gt_50ms", "max_interval_ms",
                                   "late_callbacks", "trylock_misses", "ring_samples", "ring_min_ms", "ring_max_ms",
                                   "ring_final_ms", "ring_trimmed_ms"]),
    ]
    static let diagnosticValue = "(0|[1-9][0-9]{0,19})"
    static let diagnosticFieldValues = [
        "cbl_ms": diagnosticValue + #"\.[0-9]{3}"#, "entry_ms_min": diagnosticValue + #"\.[0-9]{3}"#,
        "entry_ms_max": diagnosticValue + #"\.[0-9]{3}"#, "position_buffer": "([01])",
    ]
    static let diagnosticLines = diagnosticRecords.map { record in
        "^" + record.prefix + record.fields.map { field in
            " \(field)=\(HvfHostTail.diagnosticFieldValues[field] ?? HvfHostTail.diagnosticValue)"
        }.joined() + "$"
    }

    /// At most one of each diagnostic record, in print order, and none after a
    /// stats or continuity record.
    static func diagnosticsPlaced<S: Sequence>(_ records: S) -> Bool where S.Element: StringProtocol {
        var next = 0, teardown = false
        for record in records {
            if record.hasPrefix("hda CoreAudio stats:") || record.hasPrefix("hda CoreAudio continuity:") {
                teardown = true
            } else if let index = diagnosticRecords.firstIndex(where: { record.hasPrefix($0.prefix) }) {
                guard !teardown, index >= next else { return false }
                next = index + 1
            }
        }
        return true
    }
}

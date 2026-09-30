import Foundation

/// T17's host audio check uses B7's proven counter semantics
/// (scripts/audio-teardown-result.py): the exact ordered field set, every total
/// reconciled with its parts, and shutdown statuses the host types as expected
/// counted rather than failed. Anything unexpected, dropped or unreconciled fails.
enum T17AudioCounters {
    static let prefix = "hda CoreAudio stats: "
    static let fields = [
        "frames_rendered", "drops", "dropped_bytes", "format_drops", "ring_full_drops",
        "queue_stop_errors", "queue_dispose_errors",
        "callback_errors", "callback_active_errors", "callback_stopping_errors",
        "callback_expected_stopping_errors", "callback_unexpected_errors",
        "callback_stopping_invalid_run_state", "callback_stopping_queue_invalidated",
        "callback_stopping_enqueue_during_reset", "callback_stopping_disposal_pending",
        "callback_stopping_unclassified",
    ]

    static func parse(_ line: String) -> [String: Int]? {
        guard line.hasPrefix(prefix) else { return nil }
        let tokens = line.dropFirst(prefix.count).split(separator: " ", omittingEmptySubsequences: false)
        guard tokens.count == fields.count else { return nil }
        var values: [String: Int] = [:]
        for (token, field) in zip(tokens, fields) {
            let parts = token.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            guard parts.count == 2, parts[0] == field, !parts[1].isEmpty, parts[1].allSatisfy({ ("0"..."9").contains($0) }),
                  let value = Int(parts[1]) else { return nil }
            values[field] = value
        }
        return values
    }

    static func passed(_ line: String) -> Bool {
        guard let v = parse(line) else { return false }
        func sum(_ keys: String...) -> Int { keys.reduce(0) { $0 + v[$1]! } }
        let expected = sum("callback_stopping_invalid_run_state", "callback_stopping_enqueue_during_reset", "callback_stopping_disposal_pending")
        let reconciled = v["drops"] == sum("format_drops", "ring_full_drops")
            && v["callback_errors"] == sum("callback_active_errors", "callback_stopping_errors")
            && v["callback_expected_stopping_errors"] == expected
            && v["callback_stopping_errors"] == expected + sum("callback_stopping_queue_invalidated", "callback_stopping_unclassified")
            && v["callback_unexpected_errors"] == sum("callback_active_errors", "callback_stopping_queue_invalidated", "callback_stopping_unclassified")
        return reconciled && v["frames_rendered"]! > 0 && v["drops"] == 0 && v["queue_stop_errors"] == 0
            && v["queue_dispose_errors"] == 0 && v["callback_unexpected_errors"] == 0
            && v["callback_errors"] == v["callback_expected_stopping_errors"]
    }
}

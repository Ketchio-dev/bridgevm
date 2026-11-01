import Foundation

/// The app's pacing records of the journey's first run, for the T17 or T19 lane result. As the app's runtime
/// session ends, it replaces logs/hvf/app-pacing.log with its `app display pacing:` and `app ordered input:`
/// records (BridgeVMControl's HvfAppPacingRecords.swift). The journey marks that file before the keyboard and
/// pointer challenge and, after the first shutdown, copies the first version written since then to
/// metadata/product-e2e/first-run-app-pacing.log, waiting at most `wait` seconds for the app to end the session.
/// The copy's records join first_run_host_records as app_display_pacing and app_ordered_input
/// (scripts/live-gates/product_e2e_host_records.py admits them; hvf_app_pacing.py names the fields). A session
/// that wrote nothing, or a missing, linked, oversized or malformed file, only leaves them out: nothing gates on them.
enum T17FirstRunAppPacing {
    static let source = "logs/hvf/app-pacing.log"
    static let archive = "first-run-app-pacing.log"
    static let limit = 64 * 1024
    static let records: [(prefix: String, fields: [String])] = [
        ("app display pacing:", [
            "ticks", "presented", "repeated", "busy", "unmapped", "iosurface", "first_frames", "skipped",
            "period_min_us", "period_max_us", "missed_vsyncs", "max_tick_gap_ms",
            "hold_1", "hold_2", "hold_3", "hold_4", "hold_5", "hold_6", "hold_gt_6",
            "age_le_8ms", "age_le_17ms", "age_le_33ms", "age_le_50ms", "age_le_100ms", "age_gt_100ms",
            "age_unknown", "max_age_us", "max_work_us", "stalls_gt_50ms", "max_stall_ms",
        ]),
        ("app ordered input:", [
            "queued", "coalesced", "legacy", "refused", "sent", "inserted", "discarded",
            "depth_1", "depth_2", "depth_le_4", "depth_le_16", "depth_gt_16", "max_depth",
            "rtt_le_10ms", "rtt_le_20ms", "rtt_le_50ms", "rtt_le_100ms", "rtt_le_250ms", "rtt_gt_250ms", "max_rtt_ms",
        ]),
    ]

    /// A version of the app's file: an atomic replacement always gives it a new inode.
    struct Version: Equatable {
        let device: UInt64, inode: UInt64, size: UInt64, modified: Date?
    }

    static func mark(bundle: URL) -> Version? { version(of: bundle.appendingPathComponent(source)) }

    /// Copies the first version of the app's file other than `mark`; false when none came within `wait` seconds.
    @discardableResult
    static func archive(since mark: Version?, bundle: URL, wait: TimeInterval = 15) -> Bool {
        let file = bundle.appendingPathComponent(source), deadline = Date().addingTimeInterval(wait)
        let destination = bundle.appendingPathComponent("metadata/product-e2e/\(archive)")
        repeat {
            if let now = version(of: file), now != mark, let data = contents(file) {
                try? FileManager.default.createDirectory(at: destination.deletingLastPathComponent(),
                                                         withIntermediateDirectories: true)
                return (try? data.write(to: destination, options: .withoutOverwriting)) != nil
            }
            // A run loop without sources returns at once; sleep out the step instead of spinning.
            let step = Date().addingTimeInterval(0.2)
            RunLoop.current.run(until: step)
            if step.timeIntervalSinceNow > 0 { Thread.sleep(until: step) }
        } while Date() < deadline
        return false
    }

    static func read(bundlePath: String) -> [String: [String: UInt64]] {
        let copy = URL(fileURLWithPath: bundlePath).appendingPathComponent("metadata/product-e2e/\(archive)")
        return contents(copy).map { parse(String(decoding: $0, as: UTF8.self)) } ?? [:]
    }

    /// Both records as integer fields, or none unless `text` is exactly the two lines the app writes.
    static func parse(_ text: String) -> [String: [String: UInt64]] {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count == records.count + 1, lines.last?.isEmpty == true else { return [:] }
        var parsed: [String: [String: UInt64]] = [:]
        for (line, record) in zip(lines, records) {
            guard line.hasPrefix(record.prefix + " ") else { return [:] }
            let tokens = line.dropFirst(record.prefix.count + 1).split(separator: " ", omittingEmptySubsequences: false)
            guard tokens.count == record.fields.count else { return [:] }
            var fields: [String: UInt64] = [:]
            for (field, token) in zip(record.fields, tokens) {
                guard token.hasPrefix(field + "="), let value = number(token.dropFirst(field.count + 1)) else { return [:] }
                fields[field] = value
            }
            parsed[HvfHostRecordFields.name(record.prefix)] = fields
        }
        return parsed
    }

    /// An unsigned decimal without leading zeros that fits UInt64.
    private static func number(_ digits: Substring) -> UInt64? {
        guard !digits.isEmpty, digits.utf8.allSatisfy({ (48...57).contains($0) }),
              digits == "0" || digits.first != "0" else { return nil }
        return UInt64(digits)
    }

    /// A regular, unlinked file of at most `limit` bytes.
    private static func contents(_ url: URL) -> Data? {
        T17AppPacingFile.read(url, limit: limit)
    }

    private static func version(of url: URL) -> Version? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              attributes[.type] as? FileAttributeType == .typeRegular,
              let inode = (attributes[.systemFileNumber] as? NSNumber)?.uint64Value,
              let device = (attributes[.systemNumber] as? NSNumber)?.uint64Value,
              let size = (attributes[.size] as? NSNumber)?.uint64Value else { return nil }
        return Version(device: device, inode: inode, size: size, modified: attributes[.modificationDate] as? Date)
    }
}

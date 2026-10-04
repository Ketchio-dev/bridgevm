import Foundation

/// Return-boundary timing only; no AX reads, values or synchronous call preemption.
final class T17ChooserTiming {
    private static let stages: Set<String> = ["target-lookup", "initial-panel-check", "open-control",
        "panel-appearance", "show-location-field", "location-field-ready", "set-location",
        "accept-location", "location-sheet-dismissal", "accept-selection", "selection-confirmation"]
    private let deadline: TimeInterval
    private let now: () -> TimeInterval
    private var lastClock: TimeInterval
    private(set) var records: [String] = []
    private(set) var dropped = 0
    private(set) var lastComplete = "none"
    private(set) var clockState = "valid"

    init(start: TimeInterval, deadline: TimeInterval, now: @escaping () -> TimeInterval) {
        self.deadline = deadline; self.now = now; self.lastClock = start
        if !lastClock.isFinite || !deadline.isFinite { clockState = "nonfinite" }
    }
    func checkedNow() -> TimeInterval { (try? mark("clock-check")) ?? .infinity }
    private func mark(_ stage: String) throws -> TimeInterval {
        guard clockState == "valid" else { throw clockFailure(stage) }
        let time = now()
        if !time.isFinite || !(time * 1000).isFinite || !((time - lastClock) * 1000).isFinite || !((deadline - time) * 1000).isFinite { clockState = "nonfinite" }
        else if time < lastClock { clockState = "backwards" }
        guard clockState == "valid" else { throw clockFailure(stage) }
        lastClock = time
        return time
    }
    private func clockFailure(_ stage: String) -> T17Blocker {
        T17FileChooser.failure("stage=\(stage); chooser timing clock refused")
    }

    func run<Value>(_ stage: String, _ action: () throws -> Value) throws -> Value {
        guard Self.stages.contains(stage) else { throw T17FileChooser.failure("chooser timing stage refused") }
        let entry = try mark(stage)
        do {
            let value = try action()
            let returned = try mark(stage)
            append(stage, entry: entry, returned: returned, outcome: "returned")
            lastComplete = stage
            return value
        } catch {
            append(stage, entry: entry, returned: try? mark(stage), outcome: "threw")
            throw error
        }
    }

    private func append(_ stage: String, entry: TimeInterval, returned: TimeInterval?, outcome: String) {
        let ms = T17ChooserTimingFormat.milliseconds
        let record = "\(stage):entry_ms=\(ms(entry)),elapsed_ms=\(ms(returned.map { $0 - entry })),"
            + "remaining_ms=\(ms(returned.map { deadline - $0 })),return=\(outcome)"
        if records.count == 16 { records.removeFirst(); dropped += 1 }
        records.append(record)
    }

    var snapshot: String {
        T17ChooserTimingFormat.snapshot(header: "clock=\(clockState),last_complete=\(lastComplete),records=\(records.count),dropped=\(dropped);",
            records: records)
    }

    func attributed(_ blocker: T17Blocker) -> T17Blocker {
        T17Blocker(code: blocker.code, detail: blocker.detail + "; chooser_timing{" + snapshot + "}")
    }
}

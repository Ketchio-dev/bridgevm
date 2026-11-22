import Foundation

/// Checks identity and the shape the lifecycle ledger relies on. Reason, cause and failure strings are the
/// runner's to name: any failure code or failed outcome is a cleanup failure, never a protocol error.
enum HvfOwnedRuntimeEventValidation {
    static func validate(_ event: HvfOwnedRuntimeEvent) throws {
        guard event.schemaVersion == 1, HvfOwnedRuntimeCodec.canonicalUUID(event.runToken),
              event.runnerPID > 0, event.sequence > 0 else { throw HvfOwnedRuntimeProtocolError.invalidIdentity }
        let present = [event.ready != nil, event.stopAck != nil, event.child != nil,
                       event.unconfirmed != nil, event.complete != nil].filter { $0 }.count
        guard present == 1 else { throw HvfOwnedRuntimeProtocolError.invalidMessage }
        switch event.kind {
        case "ready":
            guard let value = event.ready, HvfOwnedRuntimeCodec.hex(value.manifestSHA256, bytes: 32),
                  value.generation == 0 else { throw invalid }
        case "stopAck":
            guard let value = event.stopAck, HvfOwnedRuntimeCodec.canonicalUUID(value.operationID),
                  value.disposition == "accepted" else { throw invalid }
        case "childStarted", "childReaped":
            guard let value = event.child else { throw invalid }
            try validateChild(value, reaped: event.kind == "childReaped")
        case "cleanupUnconfirmed":
            guard let value = event.unconfirmed, validRole(value.role, generation: value.generation),
                  value.pid > 0 else { throw invalid }
        case "complete":
            guard let value = event.complete else { throw invalid }
            try validateComplete(value)
        default: throw invalid
        }
    }

    private static var invalid: HvfOwnedRuntimeProtocolError { .invalidMessage }
    private static func validRole(_ role: String, generation: UInt64?) -> Bool {
        (role == "helper" && generation != nil) || (role == "swtpm" && generation == nil)
    }

    static func validateChild(_ value: HvfOwnedRuntimeChild, reaped: Bool) throws {
        guard value.pid > 0, validRole(value.role, generation: value.generation) else { throw invalid }
        if reaped {
            guard value.reason != nil, let status = value.status, status >= 0 else { throw invalid }
        } else if value.reason != nil || value.status != nil { throw invalid }
    }

    private static func validateComplete(_ value: HvfOwnedRuntimeComplete) throws {
        guard value.operationID.map(HvfOwnedRuntimeCodec.canonicalUUID) ?? true else { throw invalid }
        for (role, summary) in [("helper", value.helper), ("swtpm", value.swtpm)] {
            guard summary.spawnedCount == summary.reapedCount,
                  (summary.spawnedCount == 0) == (summary.last == nil),
                  role != "swtpm" || summary.spawnedCount <= 1 else { throw invalid }
            if let last = summary.last {
                try validateChild(last, reaped: true)
                guard last.role == role else { throw invalid }
            }
        }
    }
}

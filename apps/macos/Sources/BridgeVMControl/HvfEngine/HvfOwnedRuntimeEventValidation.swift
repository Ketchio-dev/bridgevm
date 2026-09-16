import Foundation

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
                  value.generation == 0, value.termMillis == 2000, value.killReapMillis == 2000 else { throw invalid }
        case "stopAck":
            guard let value = event.stopAck, HvfOwnedRuntimeCodec.canonicalUUID(value.operationID),
                  value.disposition == "accepted" else { throw invalid }
        case "childStarted", "childReaped":
            guard let value = event.child else { throw invalid }
            try validateChild(value, reaped: event.kind == "childReaped")
        case "cleanupUnconfirmed":
            guard let value = event.unconfirmed, validRole(value.role, generation: value.generation),
                  value.pid > 0, value.mediaLeaseDisposition == "retained",
                  ["waitFailed", "termFailed", "killFailed", "reapDeadline"].contains(value.reason) else { throw invalid }
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
            guard let reason = value.reason, ["exit", "signal"].contains(reason),
                  let status = value.status, status >= 0 else { throw invalid }
        } else if value.reason != nil || value.status != nil { throw invalid }
    }

    private static func validateComplete(_ value: HvfOwnedRuntimeComplete) throws {
        guard value.operationID.map(HvfOwnedRuntimeCodec.canonicalUUID) ?? true,
              ["stopRequested", "ownerEOF", "signal", "normalExit", "cycleBudget", "startupFailed", "runtimeFailed", "protocolFailed"].contains(value.cause),
              ["finished", "cancelled", "failed"].contains(value.outcome),
              ["notAdmitted", "releasedAfterReap"].contains(value.mediaLeaseDisposition),
              ["notCreated", "removed", "removeFailed"].contains(value.runtimeDirectoryDisposition) else { throw invalid }
        if value.outcome == "finished", !["normalExit", "cycleBudget"].contains(value.cause) { throw invalid }
        if value.outcome == "cancelled", !["stopRequested", "ownerEOF", "signal", "protocolFailed"].contains(value.cause) { throw invalid }
        let codes = ["mediaAdmissionFailed", "startupFailed", "helperFailed", "resetFailed",
                     "runtimeDirectoryCleanupFailed", "protocolFailed", "transportFailed", "observerInconsistent", "sequenceExhausted"]
        if let code = value.failureCode, !codes.contains(code) { throw invalid }
        guard (value.outcome == "failed") == (value.failureCode != nil),
              value.runtimeDirectoryDisposition != "removeFailed" || value.failureCode == "runtimeDirectoryCleanupFailed" else { throw invalid }
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

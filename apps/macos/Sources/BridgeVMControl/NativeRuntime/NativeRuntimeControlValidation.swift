import Foundation

extension NativeRuntimeControlCodec {
    static func validate(_ value: NativeRuntimeStopObservation, target: NativeRuntimeStopTarget) throws {
        guard NativeRuntimeCodec.validUUID(value.operationID), value.acceptedUptime.isFinite,
              value.deadlineUptime.isFinite, value.acceptedUptime >= 0,
              value.deadlineUptime >= value.acceptedUptime,
              value.deadlineUptime - value.acceptedUptime <= 192.001
        else { throw NativeRuntimeError.invalidMessage }
        if let complete = value.supervisorComplete {
            guard complete.operationID == nil || complete.operationID == value.operationID,
                  complete.helperSpawnedCount == complete.helperReapedCount,
                  complete.swtpmSpawnedCount == complete.swtpmReapedCount,
                  complete.swtpmSpawnedCount <= 1,
                  ["notAdmitted", "releasedAfterReap"].contains(complete.mediaLeaseDisposition),
                  ["notCreated", "removed", "removeFailed"].contains(complete.runtimeDirectoryDisposition),
                  complete.failureCode.map({ ["mediaAdmissionFailed", "startupFailed", "helperFailed", "resetFailed",
                    "runtimeDirectoryCleanupFailed", "protocolFailed", "transportFailed", "observerInconsistent",
                    "sequenceExhausted"].contains($0) }) ?? true
            else { throw NativeRuntimeError.invalidMessage }
            if complete.mediaLeaseDisposition == "notAdmitted" {
                guard complete.helperSpawnedCount == 0, complete.swtpmSpawnedCount == 0 else {
                    throw NativeRuntimeError.invalidMessage
                }
            }
        }
        if let exit = value.runnerExit {
            guard exit.process.token == target.runToken, exit.process.processID == target.processID,
                  ["exit", "uncaughtSignal", "unknown"].contains(exit.reason) else {
                throw NativeRuntimeError.invalidMessage
            }
        }
        switch value.phase {
        case .completed:
            guard let complete = value.supervisorComplete, value.runnerExit != nil, value.failure == nil,
                  complete.failureCode == nil, complete.runtimeDirectoryDisposition != "removeFailed" else {
                throw NativeRuntimeError.invalidMessage
            }
        case .unconfirmed:
            guard value.failure != nil else { throw NativeRuntimeError.invalidMessage }
        case .awaitingRunnerExit:
            // Exit may already be retained while the controller drains EOF or a pending guest effect.
            guard value.supervisorComplete != nil, value.failure == nil else {
                throw NativeRuntimeError.invalidMessage
            }
        case .guestGrace, .cancelling:
            guard value.failure == nil else { throw NativeRuntimeError.invalidMessage }
        }
    }
}

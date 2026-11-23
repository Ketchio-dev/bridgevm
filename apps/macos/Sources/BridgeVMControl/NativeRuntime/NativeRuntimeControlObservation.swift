import Foundation

extension NativeRuntimeStopObservation {
    init(_ value: HvfOwnedStopObservation) throws {
        if let operationID = value.supervisorComplete?.operationID, UUID(uuidString: operationID) == nil {
            throw NativeRuntimeError.invalidMessage
        }
        self.operationID = value.operationID.uuidString
        phase = value.phase
        acceptedUptime = value.acceptedUptime
        deadlineUptime = value.deadlineUptime
        supervisorComplete = value.supervisorComplete.map { receipt in
            NativeRuntimeCleanupObservation(operationID: receipt.operationID.flatMap(UUID.init(uuidString:))?.uuidString,
                helperSpawnedCount: receipt.helper.spawnedCount, helperReapedCount: receipt.helper.reapedCount,
                swtpmSpawnedCount: receipt.swtpm.spawnedCount, swtpmReapedCount: receipt.swtpm.reapedCount,
                mediaLeaseDisposition: receipt.mediaLeaseDisposition,
                runtimeDirectoryDisposition: receipt.runtimeDirectoryDisposition, failureCode: receipt.failureCode)
        }
        runnerExit = value.runnerExit.map { exit in
            let reason: String
            switch exit.reason {
            case .exit: reason = "exit"
            case .uncaughtSignal: reason = "uncaughtSignal"
            case .unknown: reason = "unknown"
            }
            return .init(process: .init(token: exit.identity.token.uuidString, processID: exit.identity.processID),
                         reason: reason, status: exit.status)
        }
        failure = value.failure
    }
}

import Foundation

@MainActor
final class NativeInstallOperationStore {
    enum Admission {
        case accepted(NativeInstallOperation)
        case existing(NativeInstallOperation)
        case refused(NativeInstallControlRefusal)
    }

    private struct Record {
        let digest: String
        let operation: NativeInstallOperation
    }
    private let capacity: Int
    private var records: [String: Record] = [:]

    init(capacity: Int = 256) { self.capacity = capacity }

    func reserve(vmID: String, digest: String, operationID: UUID,
                 now: Double = ProcessInfo.processInfo.systemUptime) -> Admission {
        if let known = records[vmID] {
            guard known.digest == digest else { return .refused(.configurationChanged) }
            if known.operation.operationID == operationID || known.operation.reservesWork {
                return .existing(known.operation)
            }
            let replacement = NativeInstallOperation(operationID: operationID,
                expectedSavedConfigurationDigest: digest, acceptedUptime: now)
            records[vmID] = .init(digest: digest, operation: replacement)
            return .accepted(replacement)
        }
        guard records.count < capacity else { return .refused(.ledgerFull) }
        let operation = NativeInstallOperation(operationID: operationID,
            expectedSavedConfigurationDigest: digest, acceptedUptime: now)
        records[vmID] = .init(digest: digest, operation: operation)
        return .accepted(operation)
    }

    func operation(vmID: String, digest: String) -> Result<NativeInstallOperation, NativeInstallControlRefusal> {
        guard let known = records[vmID] else { return .failure(.targetUnavailable) }
        guard known.digest == digest else { return .failure(.configurationChanged) }
        return .success(known.operation)
    }

    func isActive(vmID: String) -> Bool { records[vmID]?.operation.reservesWork == true }

    func removeTerminal(vmID: String) {
        guard records[vmID]?.operation.reservesWork == false else { return }
        records[vmID] = nil
    }
}

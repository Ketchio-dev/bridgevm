import Foundation

@MainActor
final class NativeInstallOperationStore {
    enum Admission {
        case accepted(NativeInstallOperation)
        case existing(NativeInstallOperation)
        case refused(NativeInstallControlRefusal)
    }

    private let capacity: Int
    /// One operation per VM ID: the app installs that VM's current saved configuration.
    var records: [String: NativeInstallOperation] = [:]

    init(capacity: Int = 256) { self.capacity = capacity }

    func reserve(vmID: String, operationID: UUID,
                 now: Double = ProcessInfo.processInfo.systemUptime) -> Admission {
        if let known = records[vmID] {
            if known.operationID == operationID || known.reservesWork { return .existing(known) }
        } else if records.count >= capacity {
            return .refused(.ledgerFull)
        }
        let operation = NativeInstallOperation(operationID: operationID, acceptedUptime: now)
        records[vmID] = operation
        return .accepted(operation)
    }

    func operation(vmID: String) -> Result<NativeInstallOperation, NativeInstallControlRefusal> {
        records[vmID].map { .success($0) } ?? .failure(.targetUnavailable)
    }

}

extension NativeInstallOperationStore {
    func isActive(vmID: String, excluding operation: NativeInstallOperation?) -> Bool {
        guard let current = records[vmID], current !== operation else { return false }
        return current.reservesWork
    }
}

extension NativeInstallOperationStore {
    /// A finished operation stays queryable while its VM is still a pending install.
    func reconcile(with configs: [VMConfig]) {
        let pending = Set(configs.filter { $0.engineKind == .hvfEngine && $0.installPending == true }.map(\.slug))
        records = records.filter { slug, operation in operation.reservesWork || pending.contains(slug) }
    }
}

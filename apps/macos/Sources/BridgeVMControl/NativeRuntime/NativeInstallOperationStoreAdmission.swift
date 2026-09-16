import Foundation

extension NativeInstallOperationStore {
    func isActive(vmID: String, excluding operation: NativeInstallOperation?) -> Bool {
        guard let current = records[vmID]?.operation, current !== operation else { return false }
        return current.reservesWork
    }
}

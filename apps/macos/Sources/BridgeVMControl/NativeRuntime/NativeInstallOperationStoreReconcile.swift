import Foundation

extension NativeInstallOperationStore {
    func reconcile(with configs: [VMConfig]) {
        var current: [String: String] = [:]
        for config in configs where config.engineKind == .hvfEngine && config.installPending == true {
            if let digest = try? NativeRuntimeConfigurationIdentity.digest(config: config) {
                current[config.slug] = digest
            }
        }
        records = records.filter { slug, record in
            record.operation.reservesWork || current[slug] == record.digest
        }
    }
}

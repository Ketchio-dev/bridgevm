import Foundation

enum FirstRunImportVTPMValidation {
    static func validate(_ inputs: FirstRunImport.Inputs, fileManager: FileManager) -> FirstRunImport.ValidationError? {
        guard let state = trimmed(inputs.vtpmStateDir) else {
            let hasRecovery = trimmed(inputs.vtpmRecoveryPackagePath) != nil || trimmed(inputs.vtpmRecoveryCodePath) != nil
            return hasRecovery ? .vtpmRecoveryWithoutState : nil
        }
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: state, isDirectory: &isDirectory), isDirectory.boolValue else {
            return .vtpmNotADirectory(state)
        }
        switch recoveryFiles(package: inputs.vtpmRecoveryPackagePath, code: inputs.vtpmRecoveryCodePath) {
        case .failure(let error): return error
        case .success(let paths):
            for path in [paths.package, paths.code] {
                var directory: ObjCBool = false
                guard fileManager.fileExists(atPath: path, isDirectory: &directory), !directory.boolValue,
                      fileManager.isReadableFile(atPath: path) else { return .vtpmRecoveryMissing(path) }
            }
        }
        return nil
    }

    static func recoveryFiles(package: String?, code: String?)
        -> Result<(package: String, code: String), FirstRunImport.ValidationError> {
        guard let package = trimmed(package) else { return .failure(.vtpmRecoveryPackageRequired) }
        guard let code = trimmed(code) else { return .failure(.vtpmRecoveryCodeRequired) }
        return .success((package, code))
    }

    static func trimmed(_ value: String?) -> String? {
        guard let value else { return nil }
        let result = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return result.isEmpty ? nil : result
    }
}

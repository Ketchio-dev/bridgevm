import Foundation

enum FirstRunImportVTPMValidation {
    static func validate(_ inputs: FirstRunImport.Inputs, fileManager: FileManager) -> FirstRunImport.ValidationError? {
        let state = trimmed(inputs.vtpmStateDir)
        let package = trimmed(inputs.vtpmRecoveryPackagePath)
        let code = trimmed(inputs.vtpmRecoveryCodePath)
        guard let state else { return package == nil && code == nil ? nil : .vtpmRecoveryWithoutState }
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: state, isDirectory: &isDirectory), isDirectory.boolValue else {
            return .vtpmNotADirectory(state)
        }
        for path in [package, code] {
            guard let path else { return .vtpmRecoveryMissing("") }
            var directory: ObjCBool = false
            guard fileManager.fileExists(atPath: path, isDirectory: &directory), !directory.boolValue,
                  fileManager.isReadableFile(atPath: path) else { return .vtpmRecoveryMissing(path) }
        }
        return nil
    }

    private static func trimmed(_ value: String?) -> String? {
        guard let value else { return nil }
        let result = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return result.isEmpty ? nil : result
    }
}

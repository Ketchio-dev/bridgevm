import Foundation

final class ImportedVTPMKeyCustody {
    private let stableID: String
    private let keyStore: VTPMStateKeyManaging
    private var preserved = false

    init(stableID: String, keyStore: VTPMStateKeyManaging) {
        self.stableID = stableID; self.keyStore = keyStore
    }
    func preserve() { preserved = true }
    deinit { if !preserved { try? keyStore.deleteStateKey(for: stableID) } }
}

enum FirstRunImportVTPMRecovery {
    static func materialize(_ inputs: FirstRunImport.Inputs, destinationStableVMID: String,
        destination: URL, fileManager: FileManager,
        keyStore: VTPMStateKeyManaging = KeychainVTPMStateKeyStore()) throws -> ImportedVTPMKeyCustody? {
        guard let source = FirstRunImportVTPMValidation.trimmed(inputs.vtpmStateDir) else { return nil }
        let paths = try FirstRunImportVTPMValidation.recoveryFiles(
            package: inputs.vtpmRecoveryPackagePath, code: inputs.vtpmRecoveryCodePath).get()
        for entry in try fileManager.contentsOfDirectory(atPath: source) where entry != ".lock" {
            try fileManager.copyItem(atPath: (source as NSString).appendingPathComponent(entry),
                toPath: destination.appendingPathComponent(entry).path)
        }
        let package = URL(fileURLWithPath: paths.package)
        let codeURL = URL(fileURLWithPath: paths.code)
        try requireBoundedRegular(package, maximum: 1_048_576)
        try requireBoundedRegular(codeURL, maximum: 4_096, ownerOnly: true)
        let code = try String(contentsOf: codeURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        try VTPMIdentityLifecycle(keyStore: keyStore).restoreImportedRecovery(
            destinationStableVMID: destinationStableVMID, stateDirectory: destination,
            packageURL: package, recoveryCode: code)
        return ImportedVTPMKeyCustody(stableID: destinationStableVMID, keyStore: keyStore)
    }

    private static func requireBoundedRegular(_ url: URL, maximum: Int, ownerOnly: Bool = false) throws {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        let mode = (try FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions] as? NSNumber)?.intValue
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= maximum, !ownerOnly || mode.map({ $0 & 0o077 == 0 }) == true else {
            throw FirstRunImport.ValidationError.vtpmRecoveryMissing(url.path)
        }
    }
}

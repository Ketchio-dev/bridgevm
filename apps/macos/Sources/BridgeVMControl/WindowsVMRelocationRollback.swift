import Foundation

extension VMLibrary {
    static func recoverWindowsHVFMove(original: VMConfig, moved: VMConfig, pending: URL,
                                     rootURL: URL, fileManager: FileManager) {
        let source = URL(fileURLWithPath: original.bundlePath, isDirectory: true)
        let destination = URL(fileURLWithPath: moved.bundlePath, isDirectory: true)
        try? fileManager.moveItem(at: destination, to: source)
        if let observed = VMRelocationRecovery.configuration(original: original, moved: moved,
                                                             fileManager: fileManager) {
            if save(observed, rootURL: rootURL) {
                try? fileManager.removeItem(at: pending)
            }
        }
    }
}

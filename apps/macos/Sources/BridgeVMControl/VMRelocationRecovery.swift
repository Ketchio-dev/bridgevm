import Foundation

enum VMRelocationRecovery {
    private enum Location { case directory, absent, uncertain }

    /// Observe location only. A present directory does not prove complete media.
    static func configuration(original: VMConfig, moved: VMConfig,
                              fileManager: FileManager = .default) -> VMConfig? {
        switch (location(original.bundlePath, fileManager), location(moved.bundlePath, fileManager)) {
        case (.directory, .absent): return original
        case (.absent, .directory): return moved
        default: return nil
        }
    }

    private static func location(_ path: String, _ fileManager: FileManager) -> Location {
        do {
            let attributes = try fileManager.attributesOfItem(atPath: path)
            return attributes[.type] as? FileAttributeType == .typeDirectory ? .directory : .uncertain
        } catch {
            let error = error as NSError
            if error.domain == NSCocoaErrorDomain,
               error.code == CocoaError.fileNoSuchFile.rawValue || error.code == CocoaError.fileReadNoSuchFile.rawValue {
                return .absent
            }
            return .uncertain
        }
    }
}

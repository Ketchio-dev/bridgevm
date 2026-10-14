import Foundation

enum LibraryInitialScan {
    static func perform(rootURL: URL, migrateLegacy: Bool) -> VMLibraryScan {
        if migrateLegacy {
            VMLibrary.migrateLegacyIfNeeded(rootURL: rootURL, legacy: VMConfig.loadLegacy())
        }
        return VMLibrary.scan(rootURL: rootURL)
    }
}

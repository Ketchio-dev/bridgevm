import Foundation

enum LibraryDeletionProtection: Equatable {
    case nativeMediaLease
    case registration

    static func mode(for config: VMConfig) -> Self {
        config.engineKind == .hvfEngine ? .nativeMediaLease : .registration
    }

    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        switch mode(for: config) {
        case .nativeMediaLease: return HvfProtectedTransfers.delete(config, rootURL: rootURL)
        case .registration: return VMLibrary.delete(config.slug, rootURL: rootURL)
        }
    }
}

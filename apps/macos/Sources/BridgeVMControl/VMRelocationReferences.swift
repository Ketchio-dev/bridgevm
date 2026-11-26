import Foundation

enum VMRelocationReferences {
    static func permits(_ config: VMConfig, rootURL: URL) -> Bool {
        config.engineKind == .hvfEngine && refusal(for: config, rootURL: rootURL) == nil
    }

    static func refusal(for config: VMConfig, rootURL: URL) -> String? {
        do {
            try LibrarySavedReferences.validate(config, rootURL: rootURL,
                affectedPath: URL(fileURLWithPath: config.bundlePath))
            return nil
        } catch {
            return "Another VM may use files in this bundle. Move that VM or check its registration before moving this one."
        }
    }

    static func failureMessage(for config: VMConfig, rootURL: URL) -> String {
        refusal(for: config, rootURL: rootURL)
            ?? "BridgeVM could not confirm the move. Check the bundles at both locations and the VM’s library entry."
    }
}

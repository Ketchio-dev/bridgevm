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
            return "Another VM in your library may be using files in this VM’s folder. Move or delete that VM first, then try again."
        }
    }

    static func failureMessage(for config: VMConfig, rootURL: URL) -> String {
        refusal(for: config, rootURL: rootURL)
            ?? "BridgeVM couldn’t confirm that the move finished. Check the VM’s original folder and the folder you chose in Finder."
    }
}

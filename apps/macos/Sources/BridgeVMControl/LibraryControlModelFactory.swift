import Foundation

@MainActor
enum LibraryControlModelFactory {
    static func make(rootURL: URL, startsAutomatically: Bool) -> @MainActor (VMConfig) -> ControlModel {
        { config in
            ControlModel(config: config, backend: config.makeBackend(libraryRoot: rootURL),
                startsAutomatically: startsAutomatically)
        }
    }
}

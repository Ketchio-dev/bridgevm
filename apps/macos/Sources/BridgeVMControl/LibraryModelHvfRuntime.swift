import Foundation

@MainActor
final class HvfRuntimeSessionStore {
    typealias Factory = @MainActor (HvfEngineConfig) -> HvfEngineSession
    private enum Key: Hashable { case experimental, libraryVM(String) }
    private struct Entry {
        let sourceConfig: VMConfig?
        let session: HvfEngineSession
    }
    private var entries: [Key: Entry] = [:]
    private let makeSession: Factory

    init(makeSession: @escaping Factory) { self.makeSession = makeSession }

    func isActive(slug: String) -> Bool {
        guard let entry = entries[.libraryVM(slug)] else { return false }
        return entry.session.connectionState != .stopped
    }

    func experimentalSession() -> HvfEngineSession {
        if let entry = entries[.experimental] { return entry.session }
        let session = makeSession(HvfEngineView.defaultConfig())
        entries[.experimental] = Entry(sourceConfig: nil, session: session)
        return session
    }

    func session(for config: VMConfig, libraryRoot: URL) -> HvfEngineSession? {
        let key = Key.libraryVM(config.slug)
        if let entry = entries[key], entry.session.connectionState != .stopped { return entry.session }
        guard let launch = HvfEngineConfig.libraryVM(config, rootURL: libraryRoot) else { return nil }
        // Compare the saved input, not the session's accepted launch options.
        if let entry = entries[key], entry.sourceConfig == config { return entry.session }
        let session = makeSession(launch)
        entries[key] = Entry(sourceConfig: config, session: session)
        return session
    }

    func owns(_ session: HvfEngineSession, for config: VMConfig) -> Bool {
        guard let entry = entries[.libraryVM(config.slug)] else { return false }
        return entry.session === session && entry.sourceConfig == config
    }

    func reconcile(with configs: [VMConfig]) {
        let runtimeSlugs = Set(configs.filter {
            $0.engineKind == .hvfEngine && $0.installPending != true
        }.map(\.slug))
        entries = entries.filter { key, entry in
            if case .experimental = key { return true }
            guard case let .libraryVM(slug) = key else { return false }
            return entry.session.connectionState != .stopped || runtimeSlugs.contains(slug)
        }
    }
}

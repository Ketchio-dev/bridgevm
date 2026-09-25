import Foundation

@MainActor
final class HvfRuntimeSessionStore {
    typealias Factory = @MainActor (HvfEngineConfig) -> HvfEngineSession
    private enum Key: Hashable { case experimental, libraryVM(String) }
    private var entries: [Key: HvfRuntimeSessionRecord] = [:]
    private let makeSession: Factory

    init(makeSession: @escaping Factory) { self.makeSession = makeSession }
    func existingRecord(slug: String) -> HvfRuntimeSessionRecord? { entries[.libraryVM(slug)] }
    func isActive(slug: String) -> Bool {
        guard let entry = entries[.libraryVM(slug)] else { return false }
        return entry.session.hasActiveRuntimeWork
    }

    func experimentalSession() -> HvfEngineSession {
        if let entry = entries[.experimental] { return entry.session }
        let session = makeSession(HvfEngineView.defaultConfig())
        entries[.experimental] = HvfRuntimeSessionRecord(sourceConfig: nil, session: session)
        return session
    }

    func session(for config: VMConfig, libraryRoot: URL, e2eHostDiagnosticStopAdmitted: Bool = false) -> HvfEngineSession? {
        let key = Key.libraryVM(config.slug)
        if let entry = entries[key], entry.session.hasActiveRuntimeWork { return entry.session }
        guard let launch = HvfEngineConfig.libraryVM(config, rootURL: libraryRoot, e2eHostDiagnosticStopAdmitted: e2eHostDiagnosticStopAdmitted) else { return nil }
        // Compare the saved input, not the session's accepted launch options.
        if let entry = entries[key], entry.sourceConfig == config { return entry.session }
        let session = makeSession(launch)
        entries[key] = HvfRuntimeSessionRecord(sourceConfig: config, session: session)
        return session
    }

    func owns(_ session: HvfEngineSession, for config: VMConfig) -> Bool {
        guard let entry = entries[.libraryVM(config.slug)] else { return false }
        return entry.session === session && entry.sourceConfig == config
    }

    func retainedControls(excludingSlugs: Set<String>) -> [LibraryRetainedControlDescriptor] {
        entries.values.compactMap { $0.retainedControl(excludingSlugs: excludingSlugs) }
    }

    func reconcile(with configs: [VMConfig]) {
        let runtimeSlugs = Set(configs.filter {
            $0.engineKind == .hvfEngine && $0.installPending != true
        }.map(\.slug))
        entries = entries.filter { key, entry in
            if case .experimental = key { return true }
            guard case let .libraryVM(slug) = key else { return false }
            return entry.session.hasActiveRuntimeWork || runtimeSlugs.contains(slug)
        }
    }
}

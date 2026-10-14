import Foundation

@MainActor
final class HvfRuntimeSessionStore {
    typealias Factory = @MainActor (HvfEngineConfig) -> HvfEngineSession
    private enum Key: Hashable { case experimental, libraryVM(String) }
    private var entries: [Key: HvfRuntimeSessionRecord] = [:]
    private let makeSession: Factory

    init(makeSession: @escaping Factory) { self.makeSession = makeSession }
    func existingRecord(slug: String) -> HvfRuntimeSessionRecord? { entries[.libraryVM(slug)] }

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

    /// A stopped session follows a registration it saved itself, so its view and unsaved edits survive.
    func rebind(_ session: HvfEngineSession, from previous: VMConfig, to saved: VMConfig) -> Bool {
        let key = Key.libraryVM(saved.slug)
        guard previous.slug == saved.slug, let entry = entries[key], entry.session === session,
              entry.sourceConfig == previous, !session.hasActiveRuntimeWork else { return false }
        entries[key] = HvfRuntimeSessionRecord(sourceConfig: saved, session: session)
        return true
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

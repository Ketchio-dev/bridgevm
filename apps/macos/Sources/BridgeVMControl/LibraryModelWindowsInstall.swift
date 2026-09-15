import Foundation

/// A library owns install work independently of the selected detail view.
@MainActor
final class HvfWindowsInstallSessionStore {
    typealias Factory = @MainActor (HvfWindowsInstallPlan) -> HvfWindowsInstallSession
    private var entries: [String: HvfWindowsInstallSessionRecord] = [:]
    private let makeSession: Factory
    let preparation: HvfWindowsInstallPreparationStore

    init(makeSession: @escaping Factory, preparation: HvfWindowsInstallPreparationOptions = .init()) {
        self.makeSession = makeSession
        self.preparation = HvfWindowsInstallPreparationStore(options: preparation)
    }

    func record(for slug: String) -> HvfWindowsInstallSessionRecord? { entries[slug] }

    func session(for config: VMConfig, request: HvfWindowsInstallRequest,
                 makePlan: () -> HvfWindowsInstallPlan) -> HvfWindowsInstallSession {
        if let entry = entries[config.slug], entry.session.isRunning { return entry.session }
        if let entry = entries[config.slug], entry.sourceConfig == config,
           entry.request == request { return entry.session }
        let session = makeSession(makePlan())
        entries[config.slug] = HvfWindowsInstallSessionRecord(sourceConfig: config, request: request, session: session)
        return session
    }

    func owns(_ session: HvfWindowsInstallSession, for config: VMConfig) -> Bool {
        guard let entry = entries[config.slug] else { return false }
        return entry.session === session && entry.sourceConfig == config
    }

    func retainedControls(excludingSlugs: Set<String>) -> [LibraryRetainedControlDescriptor] {
        entries.values.compactMap { $0.retainedControl(excludingSlugs: excludingSlugs) }
    }

    func reconcile(with configs: [VMConfig]) {
        preparation.reconcile(with: configs)
        let pendingSlugs = Set(configs.filter {
            $0.engineKind == .hvfEngine && $0.installPending == true
        }.map(\.slug))
        entries = entries.filter { slug, entry in
            entry.session.isRunning || pendingSlugs.contains(slug)
        }
    }
}

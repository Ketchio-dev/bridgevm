import Foundation

/// A library owns install work independently of the selected detail view.
@MainActor
final class HvfWindowsInstallSessionStore {
    typealias Factory = @MainActor (HvfWindowsInstallPlan) -> HvfWindowsInstallSession
    private var entries: [String: HvfWindowsInstallSessionRecord] = [:]
    private let makeSession: Factory

    init(makeSession: @escaping Factory) { self.makeSession = makeSession }

    func isActive(slug: String) -> Bool { entries[slug]?.session.isRunning == true }

    func session(for config: VMConfig, libraryRoot: URL, repoRoot: URL) -> HvfWindowsInstallSession {
        // The accepted plan stays authoritative until its work ends, even when
        // registration or request metadata changes while another view is open.
        if let entry = entries[config.slug], entry.session.isRunning { return entry.session }
        let request = HvfWindowsInstallRequest.load(bundlePath: config.bundlePath)
            ?? HvfWindowsInstallRequest(isoPath: "", diskGiB: 64,
                                       injectViogpu3d: false, driverPackageDir: nil)
        if let entry = entries[config.slug], entry.sourceConfig == config,
           entry.request == request { return entry.session }
        let plan = HvfWindowsInstallPlan(repoRoot: repoRoot, libraryRoot: libraryRoot,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
        let session = makeSession(plan)
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
        let pendingSlugs = Set(configs.filter {
            $0.engineKind == .hvfEngine && $0.installPending == true
        }.map(\.slug))
        entries = entries.filter { slug, entry in
            entry.session.isRunning || pendingSlugs.contains(slug)
        }
    }
}

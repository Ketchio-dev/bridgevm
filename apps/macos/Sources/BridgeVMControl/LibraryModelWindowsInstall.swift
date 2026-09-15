import Foundation

/// A library owns install work independently of the selected detail view.
@MainActor
final class HvfWindowsInstallSessionStore {
    typealias Factory = @MainActor (HvfWindowsInstallPlan) -> HvfWindowsInstallSession
    private struct Entry {
        let bundlePath: String
        let request: HvfWindowsInstallRequest
        let session: HvfWindowsInstallSession
    }
    private var entries: [String: Entry] = [:]
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
        if let entry = entries[config.slug], entry.bundlePath == config.bundlePath,
           entry.request == request { return entry.session }
        let plan = HvfWindowsInstallPlan(repoRoot: repoRoot, libraryRoot: libraryRoot,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
        let session = makeSession(plan)
        entries[config.slug] = Entry(bundlePath: config.bundlePath, request: request, session: session)
        return session
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

extension LibraryModel {
    func shouldShowWindowsInstall(for config: VMConfig) -> Bool {
        windowsInstallSessions.isActive(slug: config.slug)
            || (config.engineKind == .hvfEngine && config.installPending == true)
    }

    func windowsInstallSession(for config: VMConfig) -> HvfWindowsInstallSession {
        let session = windowsInstallSessions.session(for: config, libraryRoot: rootURL,
            repoRoot: HvfEngineSession.defaultRepoRoot())
        session.onCompleted = { [weak self] in self?.reload() }
        return session
    }
}

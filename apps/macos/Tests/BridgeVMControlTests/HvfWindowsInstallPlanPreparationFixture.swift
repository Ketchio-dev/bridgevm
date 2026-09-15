import XCTest
import CryptoKit
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPlanPreparationFixture {
    let work: HvfRuntimeWorkAdmissionFixture
    let probe: HvfWindowsInstallPlanPreparationProbe
    let isoBytes = Data("tiny owned plan preparation ISO\n".utf8)
    var made = 0
    private var jobs: [@MainActor () async -> Void] = []
    private var acknowledgments: [Task<Void, Never>] = []
    var jobCount: Int { jobs.count }
    var isoURL: URL { work.root.appendingPathComponent("owned.iso") }

    init(gatedCalls: Set<Int> = [], queuedInstall: Bool = false) throws {
        let owned = try HvfRuntimeWorkAdmissionFixture(validator: .init(
            error: queuedInstall ? nil : "owned validation refusal", gateFirstCall: !queuedInstall))
        work = owned
        let bytes = Data("tiny owned plan preparation ISO\n".utf8)
        let iso = owned.root.appendingPathComponent("owned.iso")
        try bytes.write(to: iso)
        let config = owned.config(pending: true)
        let request = HvfWindowsInstallRequest(isoPath: iso.path,
            isoSHA256: SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined(),
            diskGiB: 64, injectViogpu3d: false)
        let safe = HvfWindowsInstallPlan(repoRoot: owned.installRepo, libraryRoot: owned.root,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
        probe = HvfWindowsInstallPlanPreparationProbe(safePlan: safe, gatedCalls: gatedCalls)
    }

    func request(sealed: Bool = true, diskGiB: Int = 64) -> HvfWindowsInstallRequest {
        HvfWindowsInstallRequest(isoPath: isoURL.path,
            isoSHA256: sealed ? SHA256.hash(data: isoBytes).map { String(format: "%02x", $0) }.joined() : nil,
            diskGiB: diskGiB, injectViogpu3d: false)
    }

    func save(_ config: VMConfig, sealed: Bool = true, diskGiB: Int = 64) {
        work.save(config)
        XCTAssertTrue(request(sealed: sealed, diskGiB: diskGiB).save(bundlePath: config.bundlePath))
    }

    func alternateRepo() throws -> URL {
        let repo = work.root.appendingPathComponent("other-owned-repo")
        let helpers = repo.appendingPathComponent("helpers")
        try FileManager.default.createDirectory(at: helpers, withIntermediateDirectories: true)
        for name in ["wimlib-imagex", "bridgevm-catalog-verify"] {
            let path = helpers.appendingPathComponent(name)
            try Data("other owned marker; never executed\n".utf8).write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: path.path)
        }
        guard HvfWindowsWimlib.resolve(repoRoot: repo) == helpers.appendingPathComponent("wimlib-imagex").path,
              HvfWindowsCatalogVerifier.resolve(repoRoot: repo) == helpers.appendingPathComponent("bridgevm-catalog-verify").path else {
            XCTFail("Stop before compatibility Plan construction if owned helper resolution fails")
            throw CocoaError(.fileReadUnknown)
        }
        return repo
    }

    func library() -> LibraryModel {
        LibraryModel(rootURL: work.root, migrateLegacy: false, installSessionFactory: { plan in
            self.made += 1
            return HvfWindowsInstallSession(plan: plan,
                validate: { [validator = self.work.validator] in validator.validate($0) },
                schedule: { self.jobs.append($0) })
        }, runtimeSessionFactory: { config in
            self.work.runtimeCreations += 1
            return self.work.makeRuntime(config)
        }, installPreparation: .init(repoRoot: work.installRepo, builder: { [probe] in probe.build($0) }),
           actionScheduler: work.queue.enqueue, modelFactory: { config in
            self.work.modelCreations += 1
            return ControlModel(config: config, backend: self.work.backend(config), startsAutomatically: false)
        })
    }

    func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    func track(_ preparation: HvfWindowsInstallPreparation) {
        if let task = preparation.preparationTask { acknowledgments.append(task) }
    }

    func prepare(_ config: VMConfig, in library: LibraryModel) -> HvfWindowsInstallPreparation {
        let preparation = library.windowsInstallPreparation(for: config)
        track(preparation)
        return preparation
    }

    func host(in library: LibraryModel) throws -> HvfWindowsInstallPreparationView {
        let body = LibraryDetailView(library: library).body
        let hosts = values(HvfWindowsInstallPreparationView.self, in: body)
        hosts.forEach { track($0.preparation) } // Capture work before any throwing unwrap.
        if hosts.count != 1 { work.preserveForUnsettledWork = true }
        XCTAssertEqual(hosts.count, 1)
        XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
        XCTAssertTrue(values(VMDetailPanel.self, in: body).isEmpty)
        return try XCTUnwrap(hosts.first)
    }

    func ready(_ preparation: HvfWindowsInstallPreparation) throws -> HvfWindowsInstallSession {
        guard case let .ready(session) = preparation.state else {
            XCTFail("The acknowledged current preparation must be ready")
            throw CocoaError(.fileReadUnknown)
        }
        return session
    }

    func assertPreparing(_ preparation: HvfWindowsInstallPreparation) {
        guard case .preparing = preparation.state else { return XCTFail("Expected honest loading state") }
    }

    func assertFailed(_ preparation: HvfWindowsInstallPreparation) {
        guard case let .failed(message) = preparation.state else { return XCTFail("Expected stale/absent failure") }
        XCTAssertFalse(message.isEmpty)
    }

    func assertReadyHost(_ host: HvfWindowsInstallPreparationView,
                         session: HvfWindowsInstallSession) throws {
        XCTAssertTrue(try ready(host.preparation) === session)
        let view = try XCTUnwrap(values(HvfWindowsInstallView.self, in: host.body).first)
        XCTAssertTrue(view.session === session)
    }

    @discardableResult
    func entered(_ call: Int) async -> Bool {
        let entered = await probe.waitForEntry(call)
        XCTAssertTrue(entered, "The bounded owned builder must enter")
        XCTAssertEqual(probe.snapshot.invalidInputs, 0, "A wrong-root fallback invalidates this fixture")
        return entered
    }

    func withAcknowledgedWork(_ body: @MainActor () async throws -> Void) async throws {
        do { try await body() } catch { await drain(); throw error }
        await drain()
    }

    func withActive(_ session: HvfWindowsInstallSession, queued: Bool = false,
                    during body: @MainActor () async throws -> Void) async throws {
        guard let task = session.start() else { return XCTFail("Owned active fixture must accept validation") }
        acknowledgments.append(task)
        if queued { await task.value }
        else {
            let entered = await work.validator.waitForEntry()
            XCTAssertTrue(entered)
            if !entered { session.cancel(); work.validator.release(); await task.value; return }
        }
        XCTAssertTrue(session.isRunning)
        do { try await body() } catch {
            session.cancel(); work.validator.release(); await task.value; throw error
        }
        session.cancel(); work.validator.release(); await task.value
        XCTAssertFalse(work.validator.snapshot.gateTimedOut)
    }

    func drain() async {
        probe.releaseAll()
        work.validator.release()
        for task in acknowledgments { await task.value }
        acknowledgments.removeAll()
        XCTAssertEqual(probe.snapshot.finished, probe.snapshot.calls)
        XCTAssertEqual(probe.snapshot.invalidInputs, 0)
        XCTAssertFalse(probe.snapshot.gateTimedOut)
        XCTAssertFalse(work.validator.snapshot.gateTimedOut)
    }

    func clean() {
        XCTAssertTrue(acknowledgments.isEmpty, "Acknowledge every worker before removing its inputs")
        XCTAssertEqual(work.access.processLookups, 0)
        XCTAssertEqual(work.access.keyRequests, 0)
        XCTAssertEqual(work.queue.count, 0)
        jobs.removeAll() // Captured installer work has no execution method.
        work.clean()
    }
}

import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfRuntimeWorkAdmissionAccess: VTPMStateKeyProviding {
    var processLookups = 0, keyRequests = 0
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        keyRequests += 1
        XCTFail("Readiness-blocked fixture must not request a key")
        throw CocoaError(.fileReadUnknown)
    }
}

@MainActor
enum HvfRuntimeWorkAdmissionEntry: CaseIterable {
    case configuration, start, attach, silentAttach, automaticAttach
    var reports: Bool { self != .silentAttach && self != .automaticAttach }
    func invoke(_ session: HvfEngineSession) {
        switch self {
        case .configuration:
            var edited = session.config
            edited.ramMiB += 1024
            XCTAssertFalse(session.acceptStartConfiguration(edited))
        case .start: session.start()
        case .attach: XCTAssertFalse(session.attachToRunningVM())
        case .silentAttach: XCTAssertFalse(session.attachToRunningVM(reportRefusal: false))
        case .automaticAttach: XCTAssertFalse(session.attachIfStopped())
        }
    }
}

@MainActor
final class HvfRuntimeWorkAdmissionFixture {
    static let reservationReason = "This VM is being deleted, duplicated or moved. Try again when the operation finishes."
    static let ownerReason = "The selected VM changed. Refresh the library and reopen its window."
    static let missingReason = "This VM’s registration changed. Refresh the library and try again."
    static let expiredReason = "The VM library is unavailable. Return to the library and try again."
    static let runtimeReason = "The VM is running, starting or shutting down. Wait until it is fully stopped, then try again."
    static let installReason = "Windows is being installed. Finish or cancel installation before trying again."
    static let states: [HvfConnectionState] = [.booting, .connected(host: "fixture"), .stopping, .timedOut]
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("work-admission-" + UUID().uuidString)
    let queue = HvfRuntimeLibraryActionQueue()
    let access = HvfRuntimeWorkAdmissionAccess()
    let validator: HvfWindowsInstallValidationProbe
    private var installJobs: [@MainActor () async -> Void] = []
    private var backends: [String: HvfRuntimeWorkAdmissionBackend] = [:]
    var preserveForUnsettledWork = false
    var modelCreations = 0, runtimeCreations = 0, installCreations = 0
    var installJobCount: Int { installJobs.count }
    var installRepo: URL { root.appendingPathComponent("install-repo") }
    var runtimeRepo: URL { root.appendingPathComponent("absent-runtime-repo") }
    var destination: URL { root.appendingPathComponent("unexecuted-destination") }

    init(validator: HvfWindowsInstallValidationProbe = .init(error: "owned validation refusal")) throws {
        self.validator = validator
        let helpers = installRepo.appendingPathComponent("helpers")
        try FileManager.default.createDirectory(at: helpers, withIntermediateDirectories: true)
        for name in ["wimlib-imagex", "bridgevm-catalog-verify"] {
            let url = helpers.appendingPathComponent(name)
            try Data("owned marker; never executed\n".utf8).write(to: url)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
        }
        let wimlib = HvfWindowsWimlib.resolve(repoRoot: installRepo)
        let verifier = HvfWindowsCatalogVerifier.resolve(repoRoot: installRepo)
        guard wimlib == helpers.appendingPathComponent("wimlib-imagex").path,
              verifier == helpers.appendingPathComponent("bridgevm-catalog-verify").path else {
            XCTFail("Stop before Plan construction if a resolver selects anything outside the owned markers")
            throw CocoaError(.fileReadUnknown)
        }
    }

    func config(_ label: String = "vm", pending: Bool = false) -> VMConfig {
        let slug = (label + "-" + root.lastPathComponent).lowercased()
        return VMConfig(id: slug, name: label, displayName: label, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(label + "/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720,
            installPending: pending, memMiB: 4096, cpuCount: 2)
    }

    func save(_ config: VMConfig) {
        XCTAssertEqual(config.id, config.slug, "Fixture ID must already match the registration directory identity")
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        XCTAssertEqual(VMLibrary.scan(rootURL: root).configs.first { $0.slug == config.slug }, config)
    }
    func registration(_ config: VMConfig) -> URL { root.appendingPathComponent(config.slug + "/vm.json") }
    func request(_ config: VMConfig, diskGiB: Int = 64) -> HvfWindowsInstallRequest {
        HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: diskGiB, injectViogpu3d: false)
    }
    func saveRequest(_ config: VMConfig, diskGiB: Int = 64) {
        XCTAssertTrue(request(config, diskGiB: diskGiB).save(bundlePath: config.bundlePath))
    }
    func snapshot() throws -> [String: Data] {
        let paths = try FileManager.default.subpathsOfDirectory(atPath: root.path)
        var result: [String: Data] = [:]
        for path in paths {
            let url = root.appendingPathComponent(path)
            let values = try url.resourceValues(forKeys: [.isRegularFileKey])
            if values.isRegularFile == true { result[path] = try Data(contentsOf: url) }
        }
        return result
    }
    func backend(_ config: VMConfig) -> HvfRuntimeWorkAdmissionBackend {
        if let backend = backends[config.slug] { return backend }
        let backend = HvfRuntimeWorkAdmissionBackend()
        backends[config.slug] = backend
        return backend
    }
    func safeRuntimeConfig(_ config: HvfEngineConfig) -> HvfEngineConfig {
        var safe = config
        safe.swtpmBin = runtimeRepo.appendingPathComponent("absent-swtpm").path
        return safe
    }
    func makeRuntime(_ config: HvfEngineConfig) -> HvfEngineSession {
        HvfEngineSession(config: safeRuntimeConfig(config), repoRoot: runtimeRepo,
            processIsRunning: { _ in self.access.processLookups += 1; return false }, vtpmKeyProvider: access)
    }
    func library() -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
            self.installCreations += 1
            return HvfWindowsInstallSession(plan: plan, validate: { [probe = self.validator] in probe.validate($0) },
                schedule: { self.installJobs.append($0) })
        }, runtimeSessionFactory: { config in
            self.runtimeCreations += 1
            return self.makeRuntime(config)
        }, actionScheduler: queue.enqueue, modelFactory: { config in
            self.modelCreations += 1
            return ControlModel(config: config, backend: self.backend(config), startsAutomatically: false)
        })
    }
    func runtime(_ config: VMConfig, in library: LibraryModel) throws -> HvfEngineSession {
        let session = try XCTUnwrap(library.hvfRuntimeSession(for: config))
        guard assertRuntimePathsAbsent(session) else { throw CocoaError(.fileReadUnknown) }
        return session
    }
    func install(_ config: VMConfig, in library: LibraryModel) -> HvfWindowsInstallSession {
        library.windowsInstallSession(for: config, repoRoot: installRepo)
    }
    func model(_ config: VMConfig, in library: LibraryModel) -> ControlModel {
        let model = library.model(for: config)
        XCTAssertTrue(model.backend === backend(config))
        return model
    }
    @discardableResult
    func assertRuntimePathsAbsent(_ session: HvfEngineSession) -> Bool {
        let c = session.config
        var safe = true
        for path in [c.targetDiskPath, c.uefiVarsPath, c.evidenceDir, c.ctlFilePath,
                     c.vtpmStateDir ?? "", c.swtpmBin, runtimeRepo.path] {
            XCTAssertTrue(path.hasPrefix(root.path + "/"))
            XCTAssertFalse(FileManager.default.fileExists(atPath: path))
            safe = safe && path.hasPrefix(root.path + "/") && !FileManager.default.fileExists(atPath: path)
        }
        guard safe else { return false }
        let blocked = !c.readiness(repoRoot: runtimeRepo).launchReady
        XCTAssertTrue(blocked)
        XCTAssertEqual(access.keyRequests, 0)
        return blocked
    }
    func reserve(_ action: HvfRuntimeLibraryActionEntry, config: VMConfig, in library: LibraryModel) -> Bool {
        guard model(config, in: library).backend === backend(config) else { return false }
        let before = queue.count
        action.invoke(config, library: library, destination: destination)
        XCTAssertEqual(queue.count, before + 1)
        let reservations = action == .deletion ? library.deletingSlugs
            : action == .clone ? library.cloningSlugs : library.movingSlugs
        XCTAssertTrue(reservations.contains(config.slug))
        return queue.count == before + 1 && reservations.contains(config.slug)
    }
    func seedErrors(_ library: LibraryModel) {
        library.operationError = "existing operation notice"
        library.deletionError = "existing deletion notice"
        library.cloneError = "existing clone notice"
        library.moveError = "existing move notice"
    }
    func assertFileErrors(_ library: LibraryModel) {
        XCTAssertEqual(library.deletionError, "existing deletion notice")
        XCTAssertEqual(library.cloneError, "existing clone notice")
        XCTAssertEqual(library.moveError, "existing move notice")
    }
    func assertRuntimeRefused(_ session: HvfEngineSession, library: LibraryModel?,
                              reason: String, entries: [HvfRuntimeWorkAdmissionEntry] = HvfRuntimeWorkAdmissionEntry.allCases) {
        for entry in entries {
            guard assertRuntimePathsAbsent(session) else { return }
            let config = session.config, state = session.connectionState
            let events = session.events, heartbeat = session.lastHeartbeatAge
            let probes = access.processLookups, jobs = queue.count
            library?.operationError = "existing operation notice"
            entry.invoke(session)
            XCTAssertEqual(session.config, config); XCTAssertEqual(session.connectionState, state)
            XCTAssertEqual(session.events, events); XCTAssertEqual(session.lastHeartbeatAge, heartbeat)
            XCTAssertEqual(access.processLookups, probes); XCTAssertEqual(access.keyRequests, 0)
            XCTAssertEqual(queue.count, jobs)
            if let library {
                let eligible = entry != .configuration || state == .stopped
                XCTAssertEqual(library.operationError, entry.reports && eligible ? reason : "existing operation notice")
            }
            assertRuntimePathsAbsent(session)
        }
    }
    func assertInstallRefused(_ session: HvfWindowsInstallSession, library: LibraryModel?, reason: String) async {
        let stage = session.stage, started = session.startedAt, logs = session.logLines
        let calls = validator.snapshot.calls, jobs = installJobCount
        library?.operationError = "existing operation notice"
        let handle = session.start()
        XCTAssertNil(handle)
        if let handle { await handle.value }
        XCTAssertEqual(session.stage, stage); XCTAssertEqual(session.startedAt, started)
        XCTAssertEqual(session.logLines, logs); XCTAssertEqual(validator.snapshot.calls, calls)
        XCTAssertEqual(installJobCount, jobs)
        if let library {
            XCTAssertEqual(library.operationError, stage.isRunning ? "existing operation notice" : reason)
        }
    }
    func clean() {
        validator.release()
        queue.discard()
        installJobs.removeAll() // No execution API; discard breaks captured ownership cycles only.
        XCTAssertEqual(access.keyRequests, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
        if !preserveForUnsettledWork { try? FileManager.default.removeItem(at: root) }
    }
}

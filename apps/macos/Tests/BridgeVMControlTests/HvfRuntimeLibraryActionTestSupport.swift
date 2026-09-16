import Foundation
import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeLibraryActionQueue {
    private var jobs: [LibraryActionJob] = []
    var count: Int { jobs.count }
    func enqueue(_ job: @escaping LibraryActionJob) { jobs.append(job) }
    func discard() { jobs.removeAll() }
    // Deliberately no execution or removal accessor: file-action jobs cannot run in these tests.
}

final class HvfRuntimeLibraryActionBackend: VMBackend {
    struct Calls: Equatable {
        var starts = 0, completedStarts = 0, stops = 0, liveness = 0, ip = 0
        var resources = 0, resourceWrites = 0, guest = 0
    }
    private let lock = NSLock()
    private var recorded = Calls()
    private let startEntered = DispatchSemaphore(value: 0)
    private let startRelease = DispatchSemaphore(value: 0)
    private var holdStart = false
    private var startTimedOut = false
    let displayName = "fixture", kind = "fixture"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = false

    var calls: Calls { lock.lock(); defer { lock.unlock() }; return recorded }
    var gateTimedOut: Bool { lock.lock(); defer { lock.unlock() }; return startTimedOut }
    func holdFakeStart() { lock.lock(); holdStart = true; lock.unlock() }
    func releaseFakeStart() { startRelease.signal() }
    func waitForStartEntry() async -> Bool {
        await Task.detached { [startEntered] in startEntered.wait(timeout: .now() + 5) == .success }.value
    }
    private func record(_ field: WritableKeyPath<Calls, Int>) {
        lock.lock(); recorded[keyPath: field] += 1; lock.unlock()
    }
    func start() -> Bool {
        lock.lock()
        recorded.starts += 1
        let held = holdStart
        lock.unlock()
        startEntered.signal()
        let timedOut = held && startRelease.wait(timeout: .now() + 10) == .timedOut
        lock.lock()
        recorded.completedStarts += 1
        startTimedOut = startTimedOut || timedOut
        lock.unlock()
        return true // An accepted fake launch; no process or filesystem operation.
    }
    func stop() { record(\.stops) }
    func isRunning() -> Bool { record(\.liveness); return false }
    func currentIP() -> String? { record(\.ip); return nil }
    func resources() -> (memMiB: Int, cpu: Int) { record(\.resources); return (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { record(\.resourceWrites); return false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { record(\.guest); return ("", -1) }
}

@MainActor
enum HvfRuntimeLibraryActionEntry: CaseIterable {
    case requestDeletion, requestClone, deletion, clone, move

    func invoke(_ config: VMConfig, library: LibraryModel, destination: URL) {
        switch self {
        case .requestDeletion: library.requestDeletion(config)
        case .requestClone: library.requestWindowsClone(config)
        case .deletion: library.confirmDeletion(config)
        case .clone: library.cloneWindowsHVF(config, name: "captured-copy")
        case .move: library.moveWindowsHVFBundle(config, to: destination)
        }
    }

    func clearResponse(in library: LibraryModel) {
        switch self {
        case .requestDeletion: library.pendingDeletion = nil; library.deletionError = nil
        case .requestClone: library.pendingWindowsClone = nil; library.cloneError = nil
        case .deletion: library.deletionError = nil
        case .clone: library.cloneError = nil
        case .move: library.moveError = nil
        }
    }

    func error(in library: LibraryModel) -> String? {
        switch self {
        case .requestDeletion, .deletion: return library.deletionError
        case .requestClone, .clone: return library.cloneError
        case .move: return library.moveError
        }
    }

    static let finalActions: [Self] = [.deletion, .clone, .move]
}

@MainActor
final class HvfRuntimeLibraryActionFixture {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("library-action-" + UUID().uuidString)
    let queue = HvfRuntimeLibraryActionQueue()
    let validator = HvfWindowsInstallValidationProbe()
    private var installJobs: [@MainActor () async -> Void] = []
    private var backends: [String: HvfRuntimeLibraryActionBackend] = [:]
    private var files: Set<URL> = []
    var modelCreations = 0, runtimeCreations = 0, installCreations = 0, processLookups = 0
    var installJobCount: Int { installJobs.count }
    var creations: [Int] { [modelCreations, runtimeCreations, installCreations] }
    var destination: URL { root.appendingPathComponent("unexecuted-destination") }

    func config(_ slug: String = "vm", pending: Bool = false) -> VMConfig {
        VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(slug + "/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720,
            installPending: pending, memMiB: 4096, cpuCount: 2)
    }

    func save(_ config: VMConfig) {
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        files.insert(registration(config))
    }

    func saveInstallRequest(_ config: VMConfig) {
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: config.bundlePath))
        files.insert(URL(fileURLWithPath: config.bundlePath).appendingPathComponent(HvfWindowsInstallRequest.fileName))
    }

    func registration(_ config: VMConfig) -> URL {
        root.appendingPathComponent(config.slug).appendingPathComponent("vm.json")
    }

    func snapshot() throws -> [String: Data] {
        var result: [String: Data] = [:]
        for file in files where FileManager.default.fileExists(atPath: file.path) {
            result[file.path] = try Data(contentsOf: file)
        }
        return result
    }

    func backend(for config: VMConfig) -> HvfRuntimeLibraryActionBackend {
        if let existing = backends[config.slug] { return existing }
        let backend = HvfRuntimeLibraryActionBackend()
        backends[config.slug] = backend
        return backend
    }

    func library() -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
            self.installCreations += 1
            return HvfWindowsInstallSession(plan: plan, validate: { [probe = self.validator] in
                probe.validate($0)
            }, schedule: { self.installJobs.append($0) }, recovery: .init(inspect: { _ in .fresh }))
        }, runtimeSessionFactory: { config in
            self.runtimeCreations += 1
            return HvfEngineSession(config: config, repoRoot: self.root,
                processIsRunning: { _ in self.processLookups += 1; return false })
        }, actionScheduler: queue.enqueue, modelFactory: { config in
            self.modelCreations += 1
            return ControlModel(config: config, backend: self.backend(for: config), startsAutomatically: false)
        })
    }

    @discardableResult
    func cacheFake(in library: LibraryModel, config: VMConfig) -> ControlModel {
        let model = library.model(for: config)
        XCTAssertTrue(model.backend === backend(for: config))
        return model
    }

    func reservations(in library: LibraryModel) -> [Set<String>] {
        [library.deletingSlugs, library.cloningSlugs, library.movingSlugs]
    }

    func assertRefused(_ entries: [HvfRuntimeLibraryActionEntry]? = nil,
                       config: VMConfig, library: LibraryModel) {
        // A cached fake is mandatory even for a removed registration: the old baseline may accept it.
        guard library.model(for: config).backend === backend(for: config) else {
            return XCTFail("Never invoke a baseline file action through a real backend")
        }
        for entry in entries ?? HvfRuntimeLibraryActionEntry.allCases {
            entry.clearResponse(in: library)
            let calls = backend(for: config).calls
            let jobs = queue.count, owners = creations, reserved = reservations(in: library)
            entry.invoke(config, library: library, destination: destination)
            XCTAssertEqual(backend(for: config).calls, calls, "\(entry) must refuse before a backend probe")
            XCTAssertEqual(queue.count, jobs, "\(entry) must not schedule work")
            XCTAssertEqual(creations, owners)
            XCTAssertEqual(reservations(in: library), reserved)
            XCTAssertFalse(entry.error(in: library)?.isEmpty ?? true, "\(entry) must surface its refusal")
            if entry == .requestDeletion { XCTAssertNil(library.pendingDeletion) }
            if entry == .requestClone { XCTAssertNil(library.pendingWindowsClone) }
        }
    }

    func assertAccepted(_ entry: HvfRuntimeLibraryActionEntry, config: VMConfig, library: LibraryModel) {
        guard library.model(for: config).backend === backend(for: config) else {
            return XCTFail("Only a counting fake may receive an accepted baseline action")
        }
        let before = backend(for: config).calls
        let jobs = queue.count
        entry.invoke(config, library: library, destination: destination)
        XCTAssertEqual(queue.count, jobs + 1)
        let index = entry == .deletion ? 0 : entry == .clone ? 1 : 2
        XCTAssertTrue(reservations(in: library)[index].contains(config.slug))
        var expected = before
        if entry != .deletion { expected.liveness += 1 }
        XCTAssertEqual(backend(for: config).calls, expected)
    }

    func acknowledgeFakeStart(_ model: ControlModel, duringAdmission: () -> Void) async -> Bool {
        guard let backend = model.backend as? HvfRuntimeLibraryActionBackend else {
            XCTFail("The lifecycle setup must never start a real backend"); return false
        }
        let completed = DispatchSemaphore(value: 0)
        let subscription = model.$statusNote.sink { note in
            if note == "VM 부팅 중…" { completed.signal() }
        }
        defer { backend.releaseFakeStart(); subscription.cancel() }
        backend.holdFakeStart()
        model.start()
        let entered = await backend.waitForStartEntry()
        XCTAssertTrue(entered, "The counting fake must enter its finite gate")
        if entered { duringAdmission() }
        backend.releaseFakeStart()
        let acknowledged = await Task.detached {
            completed.wait(timeout: .now() + 5) == .success
        }.value
        XCTAssertTrue(acknowledged, "The fake Start must acknowledge its MainActor completion")
        XCTAssertFalse(backend.gateTimedOut)
        XCTAssertFalse(model.lifecycleBusy)
        return entered && acknowledged
    }

    func clean() {
        queue.discard()
        installJobs.removeAll() // Break queued closure/session ownership cycles without executing work.
        XCTAssertEqual(processLookups, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
        try? FileManager.default.removeItem(at: root)
    }
}

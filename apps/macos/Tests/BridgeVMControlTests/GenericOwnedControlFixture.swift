import XCTest
@testable import BridgeVMControl

@MainActor
final class GenericOwnedControlFixture {
    let root: URL
    var models: [ControlModel] = []
    var backends: [GenericOwnedControlBackend] = []
    init() throws { root = try OwnedChildRecorder.directory() }
    func config(_ name: String, kind: String = "fast-vz", bundle: URL? = nil) throws -> VMConfig {
        let bundle = bundle ?? root.appendingPathComponent(name + "/bundle")
        try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)
        return VMConfig(id: name, name: name, displayName: name, backendKind: kind,
            bootMode: kind == "fast-vz" ? "direct-kernel" : "windows-hvf", bundlePath: bundle.path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: name, displayWidth: 1280, displayHeight: 800)
    }
    func save(_ config: VMConfig) { XCTAssertTrue(VMLibrary.save(config, rootURL: root)) }
    func library() -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, modelFactory: { config in
            let backend = GenericOwnedControlBackend()
            let model = ControlModel(config: config, backend: backend, startsAutomatically: false)
            self.backends.append(backend); self.models.append(model)
            return model
        })
    }
    func clean() {
        for model in models { model.ownedRuntime?.requestStop() }
        try? FileManager.default.removeItem(at: root)
    }
    func wait(_ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(3)
        while !condition(), Date() < deadline { try? await Task.sleep(nanoseconds: 10_000_000) }
        XCTAssertTrue(condition())
    }
    func values<T>(_ type: T.Type, in value: Any, depth: Int = 0) -> [T] {
        guard depth < 40 else { return [] }
        if let value = value as? T { return [value] }
        return Mirror(reflecting: value).children.flatMap { values(type, in: $0.value, depth: depth + 1) }
    }
}

final class GenericOwnedControlBackend: VMOwnedLaunchingBackend, @unchecked Sendable {
    let displayName = "synthetic", kind = "fixture"
    let supportsGuestCommands = true, supportsPackageInstall = true
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = true
    private let lock = NSLock()
    var plan: GenericOwnedLaunchPlan? = .init(primary: .init(executable: "/bin/sleep", arguments: ["5"]))
    var resourceGate: DispatchSemaphore?
    private var resourceStarted = false
    var didBeginResourceUpdate: Bool { lock.withLock { resourceStarted } }
    private var externalRunning = true
    private var counts = [0, 0, 0, 0, 0]
    var calls: [Int] { lock.withLock { counts } }
    func ownedLaunchPlan() -> GenericOwnedLaunchPlan? { lock.withLock { counts[0] += 1; return plan } }
    func isRunning() -> Bool { lock.withLock { counts[1] += 1; return externalRunning } }
    func setExternalRunning(_ value: Bool) { lock.withLock { externalRunning = value } }
    func currentIP() -> String? { nil }
    func start() -> Bool { lock.withLock { counts[2] += 1; externalRunning = true; return true } }
    func stop() { lock.withLock { counts[3] += 1; externalRunning = false } }
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool {
        lock.withLock { resourceStarted = true }
        return resourceGate.map { $0.wait(timeout: .now() + 3) == .success } ?? true
    }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        lock.withLock { counts[4] += 1 }; return ("", 0)
    }
}

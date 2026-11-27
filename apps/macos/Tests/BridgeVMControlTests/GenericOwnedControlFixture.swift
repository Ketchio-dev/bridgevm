import XCTest
@testable import BridgeVMControl

@MainActor
final class GenericOwnedControlFixture {
    let root: URL
    init() throws {
        root = URL(fileURLWithPath: ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory())
            .appendingPathComponent("owned-runtime-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }
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
            ControlModel(config: config, backend: GenericOwnedControlBackend(), startsAutomatically: false)
        })
    }
    func clean() { try? FileManager.default.removeItem(at: root) }
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

/// Counts liveness probes; `resources()` is the library's only other backend call.
final class GenericOwnedControlBackend: VMBackend, @unchecked Sendable {
    private let lock = NSLock()
    private var externalRunning = true
    private var probes = 0
    var calls: Int { lock.withLock { probes } }
    func isRunning() -> Bool { lock.withLock { probes += 1; return externalRunning } }
    func setExternalRunning(_ value: Bool) { lock.withLock { externalRunning = value } }
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
}

import XCTest
@testable import BridgeVMControl

final class GenericOwnedDelayedChildFixture {
    let root: URL
    let recorder = OwnedChildRecorder()
    private let python: URL
    private var script: URL { root.appendingPathComponent("delayed-child.py") }

    init() throws {
        python = try NativeTestPython.executable()
        root = try OwnedChildRecorder.directory()
        do { try Data(Self.program.utf8).write(to: script) }
        catch { try? FileManager.default.removeItem(at: root); throw error }
    }

    func spec(_ role: String, output: GenericOwnedProcessOutput = .discard) -> GenericOwnedProcessSpec {
        .init(executable: python.path, arguments: [script.path, root.path, role], output: output)
    }

    func marker(_ role: String, _ event: String) -> URL { root.appendingPathComponent(role + "." + event) }
    func has(_ role: String, _ event: String) -> Bool { FileManager.default.fileExists(atPath: marker(role, event).path) }
    func release(_ role: String) throws { try Data("release".utf8).write(to: marker(role, "release")) }

    func awaitMarker(_ role: String, _ event: String) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 1.5
        while !has(role, event), ProcessInfo.processInfo.systemUptime < deadline {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        guard has(role, event) else {
            XCTFail("Synthetic child did not reach \(role).\(event)")
            throw CocoaError(.fileReadUnknown)
        }
    }

    func spawnAfterTPMReady(_ spec: GenericOwnedProcessSpec) throws -> Process {
        if spec.arguments.last == "primary" {
            let deadline = ProcessInfo.processInfo.systemUptime + 1.5
            while !has("tpm", "ready"), ProcessInfo.processInfo.systemUptime < deadline {
                Thread.sleep(forTimeInterval: 0.005)
            }
            guard has("tpm", "ready") else { throw CocoaError(.fileReadUnknown) }
        }
        return try recorder.spawn(spec)
    }

    func assertReleased(_ role: String) throws {
        XCTAssertEqual(try String(contentsOf: marker(role, "exit")), "released")
    }

    func clean() {
        try? release("primary"); try? release("tpm")
        recorder.cleanup()
        try? FileManager.default.removeItem(at: root)
    }

    private static let program = """
    import pathlib, signal, sys, time
    root, role = pathlib.Path(sys.argv[1]), sys.argv[2]
    signalled = False
    def mark(event, value):
        (root / (role + '.' + event)).write_text(value)
    def stop(signum, frame):
        global signalled
        if not signalled:
            signalled = True
            mark('signal', 'SIGTERM')
    signal.signal(signal.SIGTERM, stop)
    deadline = time.monotonic() + 2.5
    mark('ready', 'ready')
    while time.monotonic() < deadline:
        if signalled and (root / (role + '.release')).exists():
            mark('exit', 'released')
            sys.exit(0)
        time.sleep(0.005)
    mark('exit', 'timeout')
    sys.exit(3)
    """
}

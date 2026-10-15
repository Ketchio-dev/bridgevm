import XCTest
import Darwin
@testable import BridgeVMControl

final class GenericOwnedRuntimeTests: XCTestCase {
    func testStopSignalsOnlyOwnedChildAndCannotAdoptReplacement() async throws {
        let recorder = OwnedChildRecorder()
        let spec = sleepSpec
        let decoy = try GenericOwnedProcessLauncher.launch(spec)
        defer { recorder.cleanup(); stop(decoy) }
        let group = GenericOwnedRuntime(plan: .init(primary: spec), spawn: recorder.spawn,
                                        isDependencyRunning: { _ in false })
        await wait { group.snapshot.primaryRunning }
        XCTAssertTrue(decoy.isRunning)
        group.requestStop()
        await wait { !group.snapshot.isActive }
        XCTAssertTrue(decoy.isRunning)
        let replacement = try GenericOwnedProcessLauncher.launch(spec)
        defer { stop(replacement) }
        group.requestStop()
        XCTAssertTrue(replacement.isRunning)
        XCTAssertFalse(group.snapshot.primaryRunning)
    }

    func testCancellationDuringTPMDelayNeverCreatesPrimary() async throws {
        let recorder = OwnedChildRecorder()
        defer { recorder.cleanup() }
        let plan = GenericOwnedLaunchPlan(primary: sleepSpec, tpm: .init(process: sleepSpec,
            reusePattern: "synthetic-only", launchDelay: 2))
        let group = GenericOwnedRuntime(plan: plan, spawn: recorder.spawn, isDependencyRunning: { _ in false })
        await wait { recorder.processes.count == 1 }
        group.requestStop()
        await wait { !group.snapshot.isActive }
        XCTAssertEqual(recorder.processes.count, 1)
        XCTAssertFalse(recorder.processes[0].isRunning)
    }

    func testReusedTPMRemainsOutsideOwnedCleanup() async throws {
        let recorder = OwnedChildRecorder()
        let external = try GenericOwnedProcessLauncher.launch(sleepSpec)
        defer { recorder.cleanup(); stop(external) }
        let plan = GenericOwnedLaunchPlan(primary: sleepSpec, tpm: .init(process: sleepSpec,
            reusePattern: "synthetic-only", launchDelay: 0.01))
        let group = GenericOwnedRuntime(plan: plan, spawn: recorder.spawn, isDependencyRunning: { _ in true })
        await wait { group.snapshot.primaryRunning }
        group.requestStop()
        await wait { !group.snapshot.isActive }
        XCTAssertEqual(recorder.processes.count, 1)
        XCTAssertTrue(external.isRunning)
    }

    func testFIFOLogRefusesWithoutBlockingStopAndCleansOnlyOwnedTPM() async throws {
        let root = try OwnedChildRecorder.directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let fifo = root.appendingPathComponent("qemu.log")
        XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
        let recorder = OwnedChildRecorder()
        defer { recorder.cleanup() }
        let plan = GenericOwnedLaunchPlan(primary: .init(executable: "/bin/sleep", arguments: ["5"], output: .file(fifo.path)),
            tpm: .init(process: sleepSpec, reusePattern: "synthetic-only", launchDelay: 0.01))
        let group = GenericOwnedRuntime(plan: plan, spawn: recorder.spawn, isDependencyRunning: { _ in false })
        await wait { !group.snapshot.isActive }
        XCTAssertNotNil(group.snapshot.failure)
        XCTAssertEqual(recorder.processes.count, 1)
        XCTAssertFalse(recorder.processes[0].isRunning)
        group.requestStop()
        var info = stat()
        XCTAssertEqual(lstat(fifo.path, &info), 0)
        XCTAssertEqual(info.st_mode & S_IFMT, S_IFIFO)
    }

    func testDroppingHandleDoesNotTerminateLaunchedChild() async throws {
        let recorder = OwnedChildRecorder()
        defer { recorder.cleanup() }
        var group: GenericOwnedRuntime? = GenericOwnedRuntime(plan: .init(primary: sleepSpec), spawn: recorder.spawn,
                                                             isDependencyRunning: { _ in false })
        await wait { group?.snapshot.primaryRunning == true }
        weak var observation = group
        group = nil
        await wait { observation == nil }
        XCTAssertTrue(recorder.processes[0].isRunning)
    }

    private var sleepSpec: GenericOwnedProcessSpec { .init(executable: "/bin/sleep", arguments: ["5"]) }
    private func stop(_ process: Process) { OwnedChildRecorder.stop(process) }
    private func wait(_ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(3)
        while !condition(), Date() < deadline { try? await Task.sleep(nanoseconds: 10_000_000) }
        XCTAssertTrue(condition())
    }
}

final class OwnedChildRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var children: [Process] = []
    var processes: [Process] { lock.withLock { children } }
    func spawn(_ spec: GenericOwnedProcessSpec) throws -> Process {
        let child = try GenericOwnedProcessLauncher.launch(spec)
        lock.withLock { children.append(child) }
        return child
    }
    func cleanup() {
        for child in processes { Self.stop(child) }
    }
    static func stop(_ child: Process) {
        if child.isRunning { child.terminate() }
        let deadline = Date().addingTimeInterval(6)
        while child.isRunning, Date() < deadline { Thread.sleep(forTimeInterval: 0.01) }
        XCTAssertFalse(child.isRunning, "Synthetic child did not exit within its finite lifetime")
    }
    static func directory() throws -> URL {
        let root = URL(fileURLWithPath: ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory())
            .appendingPathComponent("owned-runtime-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }
}

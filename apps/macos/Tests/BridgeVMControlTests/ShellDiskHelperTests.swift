import XCTest
@testable import BridgeVMControl

/// A snapshot or export from the CLI runs BridgeVM's disk helper with the VM's disk on its command
/// line. `pgrep -f` took it for the VM: the library showed a stopped VM as Running, Start attached
/// to the helper, and a stop could `pkill` it in the middle of a copy.
final class ShellDiskHelperTests: XCTestCase {
    func testTheDiskHelperIsNotARunningVMAndIsNeverKilledAsOne() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("disk-helper-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let disk = root.appendingPathComponent("disk.raw").path, vars = root.appendingPathComponent("vars.fd").path
        try Data(repeating: 7, count: 4096).write(to: URL(fileURLWithPath: disk))
        try Data(repeating: 9, count: 64).write(to: URL(fileURLWithPath: vars))

        let lease = try HvfMediaLeaseSession(executable: HvfMediaImportTestSupport.helper, disk: disk, vars: vars)
        defer { lease.abort() }
        XCTAssertTrue(Shell.isDiskHelperRunning(matching: disk))
        XCTAssertFalse(Shell.isProcessRunning(matching: disk), "the helper is not the VM")

        // Any other process naming the disk still counts as the VM, and only it is stopped.
        let other = Process()
        other.executableURL = URL(fileURLWithPath: "/usr/bin/tail")
        other.arguments = ["-f", disk]
        other.standardOutput = FileHandle.nullDevice
        try other.run()
        defer { if other.isRunning { other.terminate() } }
        XCTAssertTrue(Shell.isProcessRunning(matching: disk))
        Shell.killProcesses(matching: disk)
        other.waitUntilExit()
        XCTAssertTrue(Shell.isDiskHelperRunning(matching: disk), "the helper survives a stop")
        try lease.finish()
        XCTAssertFalse(Shell.isDiskHelperRunning(matching: disk))
    }

    /// The owned runner has no disk path on its command line: after the app crashed, the reopened app
    /// showed a VM still shutting down as Stopped and started a second run against its held disk.
    func testARunnerHoldingTheDiskCountsAsARunningVM() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("disk-runner-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let disk = root.appendingPathComponent("disk.raw").path, lock = disk + ".bridgevm-writer.lock"
        try Data().write(to: URL(fileURLWithPath: disk))
        // A stand-in runner named hvf-runner, with no disk path in its arguments. Built here: a re-signed
        // copy of /bin/sleep is arm64e outside the platform, which macOS 15 kills at launch.
        let runner = root.appendingPathComponent("hvf-runner"), source = root.appendingPathComponent("runner.c")
        try Data("#include <unistd.h>\nint main(void) { pause(); return 0; }\n".utf8).write(to: source)
        XCTAssertEqual(Shell.run("/usr/bin/cc", ["-o", runner.path, source.path], timeout: 120).code, 0)
        let process = Process()
        process.executableURL = runner
        try process.run()
        defer { if process.isRunning { process.terminate() } }

        XCTAssertFalse(Shell.isProcessRunning(matching: disk))
        try Data("hvf-runner --owned-runtime-stdio (pid \(process.processIdentifier))\n".utf8).write(to: URL(fileURLWithPath: lock))
        XCTAssertTrue(Shell.isProcessRunning(matching: disk))
        // Another live process's pid, left in a lock by a killed runner, is not a runner.
        try Data("hvf-runner --owned-runtime-stdio (pid \(getpid()))\n".utf8).write(to: URL(fileURLWithPath: lock))
        XCTAssertFalse(Shell.isProcessRunning(matching: disk))
        try Data("hvf-runner --owned-runtime-stdio (pid \(process.processIdentifier))\n".utf8).write(to: URL(fileURLWithPath: lock))
        Shell.killProcesses(matching: disk)
        process.waitUntilExit()
        XCTAssertEqual(process.terminationReason, .uncaughtSignal)
        XCTAssertFalse(Shell.isProcessRunning(matching: disk), "a dead runner's pid does not count")
    }

    @MainActor
    func testFileActionsWaitForTheDiskHelperWithASentence() throws {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let config = fixture.config()
        fixture.save(config)
        let library = fixture.library()
        fixture.cacheFake(in: library, config: config)
        fixture.backend(for: config).diskHelper = true
        // The menu asks before it opens the folder panel, not after a folder was picked.
        try FileManager.default.createDirectory(atPath: config.bundlePath, withIntermediateDirectories: true)
        XCTAssertFalse(library.admitBundleMove(config))
        XCTAssertEqual(library.moveError, VMDiskHelper.busy)
        library.moveError = nil
        library.confirmDeletion(config)
        library.cloneWindowsHVF(config, name: "copy")
        library.moveWindowsHVFBundle(config, to: fixture.destination)
        XCTAssertEqual([library.deletionError, library.cloneError, library.moveError], Array(repeating: VMDiskHelper.busy, count: 3))
        XCTAssertEqual(fixture.queue.count, 0)
        XCTAssertEqual(fixture.reservations(in: library), [[], [], []])
    }
}

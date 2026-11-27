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

    @MainActor
    func testFileActionsWaitForTheDiskHelperWithASentence() {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let config = fixture.config()
        fixture.save(config)
        let library = fixture.library()
        fixture.cacheFake(in: library, config: config)
        fixture.backend(for: config).diskHelper = true
        library.confirmDeletion(config)
        library.cloneWindowsHVF(config, name: "copy")
        library.moveWindowsHVFBundle(config, to: fixture.destination)
        XCTAssertEqual([library.deletionError, library.cloneError, library.moveError], Array(repeating: VMDiskHelper.busy, count: 3))
        XCTAssertEqual(fixture.queue.count, 0)
        XCTAssertEqual(fixture.reservations(in: library), [[], [], []])
    }
}

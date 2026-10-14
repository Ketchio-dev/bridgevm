import XCTest
import Darwin
@testable import BridgeVMControl

final class GenericOwnedProcessLauncherTests: XCTestCase {
    func testChildInheritsEnvironmentAndExplicitOverridesWin() async throws {
        let root = try OwnedChildRecorder.directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let inheritedPath = try XCTUnwrap(ProcessInfo.processInfo.environment["PATH"])
        let inheritedTemporaryDirectory = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let override = "owned-launch-override"
        XCTAssertNotEqual(inheritedTemporaryDirectory, override)
        let output = root.appendingPathComponent("environment.log")
        let child = try GenericOwnedProcessLauncher.launch(.init(executable: "/bin/sh",
            arguments: ["-c", "printf '%s\\n%s' \"$PATH\" \"$TMPDIR\""],
            environmentOverrides: ["TMPDIR": override], output: .file(output.path)))
        await finish(child)
        XCTAssertEqual(try String(contentsOf: output), inheritedPath + "\n" + override)
    }

    func testCombinedOutputTruncatesOldLogAndPreservesPermissions() async throws {
        let root = try OwnedChildRecorder.directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let output = root.appendingPathComponent("existing.log")
        try Data(repeating: 0x61, count: 4096).write(to: output)
        try FileManager.default.setAttributes([.posixPermissions: 0o640], ofItemAtPath: output.path)
        let child = try GenericOwnedProcessLauncher.launch(.init(executable: "/bin/sh",
            arguments: ["-c", "printf 'out-one\\n'; printf 'err-two\\n' >&2; printf 'out-three\\n'"],
            output: .file(output.path)))
        await finish(child)
        XCTAssertEqual(try String(contentsOf: output), "out-one\nerr-two\nout-three\n")
        XCTAssertEqual(try mode(output) & 0o777, 0o640)
    }

    func testNewLogHasPrivatePermissions() async throws {
        let root = try OwnedChildRecorder.directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let output = root.appendingPathComponent("new.log")
        XCTAssertFalse(FileManager.default.fileExists(atPath: output.path))
        let child = try GenericOwnedProcessLauncher.launch(.init(executable: "/usr/bin/printf",
            arguments: ["%s", "owned output"], output: .file(output.path)))
        await finish(child)
        XCTAssertEqual(try String(contentsOf: output), "owned output")
        XCTAssertEqual(try mode(output) & 0o777, 0o600)
    }

    func testSymlinkOutputRefusesBeforeTruncatingTargetOrLaunchingChild() async throws {
        let root = try OwnedChildRecorder.directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("preserved.log")
        let link = root.appendingPathComponent("linked.log")
        let bytes = Data("preserve every byte".utf8)
        try bytes.write(to: target)
        try FileManager.default.setAttributes([.posixPermissions: 0o640], ofItemAtPath: target.path)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        await assertRefused(link, marker: root.appendingPathComponent("unexpected-child"))
        XCTAssertEqual(try Data(contentsOf: target), bytes)
        XCTAssertEqual(try mode(target) & 0o777, 0o640)
        XCTAssertEqual(try mode(link) & S_IFMT, S_IFLNK)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: link.path), target.path)
    }

    func testFIFOAndOtherSpecialOutputsRefuseBeforeLaunchingChild() async throws {
        let root = try OwnedChildRecorder.directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let fifo = root.appendingPathComponent("fifo.log")
        XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
        // A reader keeps an accidentally blocking writer-open finite and exercises fstat's type check.
        let reader = open(fifo.path, O_RDONLY | O_NONBLOCK | O_CLOEXEC)
        guard reader >= 0 else { return XCTFail("Cannot open the owned FIFO reader") }
        defer { close(reader) }
        let directory = root.appendingPathComponent("directory.log")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        let outputs = [fifo, directory, URL(fileURLWithPath: "/dev/null")]
        for (index, output) in outputs.enumerated() {
            let before = try mode(output)
            await assertRefused(output, marker: root.appendingPathComponent("unexpected-child-\(index)"))
            XCTAssertEqual(try mode(output), before)
        }
        XCTAssertEqual(try mode(fifo) & S_IFMT, S_IFIFO)
        XCTAssertEqual(try mode(directory) & S_IFMT, S_IFDIR)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: directory.path), [])
    }

    private func assertRefused(_ output: URL, marker: URL) async {
        var refused = false
        do {
            let child = try GenericOwnedProcessLauncher.launch(.init(executable: "/bin/sh",
                arguments: ["-c", "printf 'unexpected output'; printf spawned > \"$1\"", "owned-refusal", marker.path],
                output: .file(output.path)))
            await finish(child)
        } catch { refused = true }
        XCTAssertTrue(refused, "Special output must refuse before spawning a child")
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))
    }

    private func mode(_ url: URL) throws -> mode_t {
        var information = stat()
        guard lstat(url.path, &information) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        return information.st_mode
    }

    private func finish(_ child: Process) async {
        await wait(child, seconds: 2)
        let exitedNormally = !child.isRunning
        if child.isRunning { child.terminate(); await wait(child, seconds: 0.5) }
        if child.isRunning { kill(child.processIdentifier, SIGKILL); await wait(child, seconds: 1) }
        XCTAssertTrue(exitedNormally, "The finite synthetic child must exit without cleanup signals")
        XCTAssertFalse(child.isRunning, "Owned child cleanup exceeded its finite deadline")
        if !child.isRunning { XCTAssertEqual(child.terminationStatus, 0) }
    }

    private func wait(_ child: Process, seconds: TimeInterval) async {
        let deadline = ProcessInfo.processInfo.systemUptime + seconds
        while child.isRunning, ProcessInfo.processInfo.systemUptime < deadline {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
    }
}

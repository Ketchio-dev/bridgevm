import Darwin
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfLiveInputFileTests: XCTestCase {
    func testSymlinkChannelDoesNotAppendToUnrelatedFile() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let unrelated = root.appendingPathComponent("unrelated")
        let saved = Data("keep these bytes".utf8)
        try saved.write(to: unrelated)
        let input = root.appendingPathComponent("input.ctl")
        try FileManager.default.createSymbolicLink(at: input, withDestinationURL: unrelated)
        let session = session(root)
        session.sendKey("esc")
        XCTAssertEqual(try Data(contentsOf: unrelated), saved)
        XCTAssertTrue(session.events.contains { $0.displayText.contains("live input write failed:") })
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: input.path), unrelated.path)
    }

    func testReplacedChannelDoesNotReceiveOrLoseEventsThroughStaleHandle() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let input = root.appendingPathComponent("input.ctl")
        let retired = root.appendingPathComponent("retired-input")
        let session = session(root)
        session.sendKey("esc")
        try FileManager.default.moveItem(at: input, to: retired)
        let retiredBytes = try Data(contentsOf: retired)
        try Data("replacement".utf8).write(to: input)
        session.sendKey("delete")
        session.sendKey("enter")
        XCTAssertEqual(try Data(contentsOf: retired), retiredBytes)
        XCTAssertEqual(try Data(contentsOf: input), Data("replacement".utf8))
        XCTAssertTrue(session.events.contains { $0.displayText.contains("live input write failed:") })
    }

    func testHardlinkedAndSpecialChannelsRefuseWithoutMutationOrBlocking() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let victim = root.appendingPathComponent("victim")
        let input = root.appendingPathComponent("input.ctl")
        let saved = Data("keep".utf8)
        try saved.write(to: victim)
        XCTAssertEqual(link(victim.path, input.path), 0)
        let writer = HvfLiveInputFile()
        XCTAssertThrowsError(try writer.append(Data("input".utf8), to: input))
        XCTAssertEqual(try Data(contentsOf: victim), saved)
        try FileManager.default.removeItem(at: input)
        XCTAssertEqual(mkfifo(input.path, 0o600), 0)
        let began = ProcessInfo.processInfo.systemUptime
        XCTAssertThrowsError(try writer.append(Data("input".utf8), to: input))
        XCTAssertLessThan(ProcessInfo.processInfo.systemUptime - began, 2)
        try FileManager.default.removeItem(at: input)
        try FileManager.default.createDirectory(at: input, withIntermediateDirectories: false)
        XCTAssertThrowsError(try writer.append(Data("input".utf8), to: input))
    }

    func testOwnedCreationCompactionAndExplicitLifecycleReset() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let input = root.appendingPathComponent("input.ctl")
        let writer = HvfLiveInputFile()
        try writer.append(Data("first".utf8), to: input)
        var metadata = stat()
        XCTAssertEqual(lstat(input.path, &metadata), 0)
        XCTAssertEqual(metadata.st_mode & 0o777, 0o600)
        let consumer = try FileHandle(forWritingTo: input)
        try consumer.truncate(atOffset: 0)
        try consumer.close()
        try writer.append(Data("second".utf8), to: input)
        XCTAssertEqual(try Data(contentsOf: input), Data("second".utf8))
        try FileManager.default.moveItem(at: input, to: root.appendingPathComponent("retired"))
        try Data("new".utf8).write(to: input)
        XCTAssertThrowsError(try writer.append(Data("blocked".utf8), to: input))
        writer.close()
        try writer.append(Data("-session".utf8), to: input)
        XCTAssertEqual(try Data(contentsOf: input), Data("new-session".utf8))
    }

    func testPinnedChannelRefusesAliasesAndRecoversOnlyItsOriginalInode() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let input = root.appendingPathComponent("input.ctl")
        let retired = root.appendingPathComponent("retired")
        let writer = HvfLiveInputFile()
        try writer.append(Data("owned".utf8), to: input)
        try FileManager.default.moveItem(at: input, to: retired)
        try FileManager.default.createSymbolicLink(at: input, withDestinationURL: retired)
        XCTAssertThrowsError(try writer.append(Data("blocked".utf8), to: input))
        try FileManager.default.removeItem(at: input)
        XCTAssertThrowsError(try writer.append(Data("blocked".utf8), to: input))
        XCTAssertFalse(FileManager.default.fileExists(atPath: input.path))
        try FileManager.default.moveItem(at: retired, to: input)
        XCTAssertEqual(link(input.path, retired.path), 0)
        XCTAssertThrowsError(try writer.append(Data("blocked".utf8), to: input))
        try FileManager.default.removeItem(at: retired)
        try writer.append(Data("-recovered".utf8), to: input)
        XCTAssertEqual(try Data(contentsOf: input), Data("owned-recovered".utf8))
    }

    func testDanglingSymlinkDoesNotCreateItsDestination() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("absent")
        let input = root.appendingPathComponent("input.ctl")
        try FileManager.default.createSymbolicLink(at: input, withDestinationURL: target)
        let writer = HvfLiveInputFile()
        XCTAssertThrowsError(try writer.append(Data("blocked".utf8), to: input))
        XCTAssertFalse(FileManager.default.fileExists(atPath: target.path))
    }

    func testDifferentPathCannotResetARejectedIdentity() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let first = root.appendingPathComponent("input.ctl")
        let second = root.appendingPathComponent("other.ctl")
        let writer = HvfLiveInputFile()
        try writer.append(Data("old".utf8), to: first)
        try FileManager.default.moveItem(at: first, to: root.appendingPathComponent("retired"))
        try Data("new".utf8).write(to: first)
        XCTAssertThrowsError(try writer.append(Data("bad".utf8), to: first))
        XCTAssertThrowsError(try writer.append(Data("other".utf8), to: second))
        XCTAssertThrowsError(try writer.append(Data("bad".utf8), to: first))
        XCTAssertEqual(try Data(contentsOf: first), Data("new".utf8))
        XCTAssertFalse(FileManager.default.fileExists(atPath: second.path))
    }

    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("live-input-identity-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        return root
    }

    private func session(_ root: URL) -> HvfEngineSession {
        let config = HvfEngineConfig(
            targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        return HvfEngineSession(config: config, repoRoot: root) { _ in false }
    }
}

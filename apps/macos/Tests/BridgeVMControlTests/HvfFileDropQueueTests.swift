import Foundation
#if canImport(AppKit)
import AppKit
#endif
import XCTest
@testable import BridgeVMControl

/// Drops follow the runtime's `BVAGENT DROP` lines (agent_console/file_drop.rs) to one notice.
@MainActor
final class HvfFileDropQueueTests: XCTestCase {
    private func scratch() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("bvdrop-queue-\(UUID().uuidString)")
    }

    private func summary(files: Int, bytes: Int64, skipped: Int = 0) -> HvfFileDropStaging.Summary {
        var summary = HvfFileDropStaging.Summary()
        summary.files = files
        summary.bytes = bytes
        summary.skipped = skipped
        return summary
    }

    private func session(root: URL) -> HvfEngineSession {
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("metadata/hvf.ctl").path)
        return HvfEngineSession(config: config, repoRoot: root) { _ in false }
    }

    func testRuntimeLinesBecomeProgressThenDoneAndTheStagingFolderGoes() throws {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let queue = HvfFileDropQueue()
        let job = queue.begin(in: root)
        try FileManager.default.createDirectory(at: job.staging, withIntermediateDirectories: true)
        XCTAssertEqual(queue.notice?.text, "Preparing to copy…")
        queue.staged(job.id, summary(files: 2, bytes: 3000, skipped: 1), sent: true)
        queue.consume(.unknown("BVAGENT DROP start \(job.id) files=2 bytes=3000 t=10"))
        XCTAssertEqual(queue.notice?.text.hasPrefix("Copying to Windows · 0 of 2 files · 0 B / "), true)
        queue.consume(.unknown("BVAGENT DROP progress \(job.id) files=1/2 bytes=2000/3000 t=20\r"))
        XCTAssertEqual(queue.notice?.text.hasPrefix("Copying to Windows · 1 of 2 files · "), true)
        queue.consume(.unknown("BVAGENT DROP done \(job.id) files=2 bytes=3000 ms=5 chunks=2 chunk_avg_us=9 t=30"))
        XCTAssertEqual(queue.notice?.failed, false)
        XCTAssertEqual(queue.notice?.text.hasPrefix("Copied to the Windows desktop (2 files, "), true)
        XCTAssertEqual(queue.notice?.text.hasSuffix("Skipped 1 unsupported item (symbolic links or special files)."), true)
        XCTAssertFalse(FileManager.default.fileExists(atPath: job.staging.path))
    }

    func testSecondIdenticalCopyKeepsItsOwnCompletionNotice() async throws {
        let queue = HvfFileDropQueue()
        let root = scratch()
        let expected = HvfFileDropQueue.Notice(text: "Copied to the Windows desktop (1 file, 0 B).", failed: false)

        // Both drops finish on the main actor before the first dismissal runs.
        for duration: TimeInterval in [0, 8] {
            queue.finishedNoticeSeconds = duration
            let job = queue.begin(in: root)
            queue.staged(job.id, summary(files: 1, bytes: 0), sent: true)
            queue.consume(.unknown("BVAGENT DROP done \(job.id) files=1 bytes=0"))
            XCTAssertEqual(queue.notice, expected)
        }
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(queue.notice, expected)
    }

    func testEmptyFileShowsZeroBytesDuringCopyAndAtCompletion() {
        let queue = HvfFileDropQueue()
        let job = queue.begin(in: scratch())
        queue.staged(job.id, summary(files: 1, bytes: 0), sent: true)

        queue.consume(.unknown("BVAGENT DROP start \(job.id) files=1 bytes=0 t=10"))
        XCTAssertEqual(queue.notice?.text, "Copying to Windows · 0 of 1 file · 0 B / 0 B")
        queue.consume(.unknown("BVAGENT DROP done \(job.id) files=1 bytes=0 t=20"))
        XCTAssertEqual(queue.notice?.text, "Copied to the Windows desktop (1 file, 0 B).")
    }

    func testASecondDropWaitsBehindTheFirstAndEachFailureSaysWhy() {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let queue = HvfFileDropQueue()
        let first = queue.begin(in: root)
        let second = queue.begin(in: root)
        queue.staged(first.id, summary(files: 1, bytes: 10), sent: true)
        queue.staged(second.id, summary(files: 1, bytes: 10), sent: true)
        queue.consume(.unknown("BVAGENT DROP start \(first.id) files=1 bytes=10 t=1"))
        XCTAssertEqual(queue.notice?.text.hasSuffix(" · 1 queued"), true)
        queue.consume(.unknown("BVAGENT DROP start \(second.id) files=1 bytes=10 t=2 extra"))
        queue.consume(.unknown("BVAGENT DROP failed \(first.id) agent-too-old files=0/1 bytes=0/10 t=3"))
        XCTAssertEqual(queue.notice?.text.hasPrefix("Copying to Windows · 0 of 1 file"), true)
        queue.consume(.unknown("BVAGENT DROP failed \(second.id) guest-error bad-name files=0/1 bytes=0/10 t=4"))
        let failure = HvfFileDropQueue.failureText("guest-error")
            + "\nAn earlier transfer needs attention. " + HvfFileDropQueue.failureText("agent-too-old")
        XCTAssertEqual(queue.notice, .init(text: failure, failed: true))
    }

    func testQueuedSuccessReportsEarlierFailedFileAndNextCopyStartsFresh() async throws {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let session = session(root: root)
        session.serviceStarted = true
        session.connectionState = .connected(host: "WIN")
        func submit(_ name: String) async throws -> String {
            let item = root.appendingPathComponent(name)
            try Data().write(to: item)
            let staging = try XCTUnwrap(session.copyDroppedFiles([item]))
            await staging.value
            let ctl = try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
            let line = try XCTUnwrap(ctl.split(separator: "\n").last)
            let fields = line.split(separator: " ")
            XCTAssertEqual(fields.first, "BVDROP")
            return String(try XCTUnwrap(fields.dropFirst().first))
        }

        let first = try await submit("보고서.txt")
        let second = try await submit("notes.txt")
        session.append(.unknown("BVAGENT DROP failed \(first) guest-error files=0/1 bytes=0/0"))
        session.append(.unknown("BVAGENT DROP start \(second) files=1 bytes=0"))
        XCTAssertEqual(session.fileDrops.notice?.text, "Copying to Windows · 0 of 1 file · 0 B / 0 B")
        session.append(.unknown("BVAGENT DROP done \(second) files=1 bytes=0"))
        let copied = "Copied to the Windows desktop (1 file, 0 B)."
        let earlier = "The earlier transfer of “보고서.txt” needs attention. " + HvfFileDropQueue.failureText("guest-error")
        XCTAssertEqual(session.fileDrops.notice, .init(text: copied + "\n" + earlier, failed: true))

        let independent = try await submit("next.txt")
        session.append(.unknown("BVAGENT DROP done \(independent) files=1 bytes=0"))
        XCTAssertEqual(session.fileDrops.notice, .init(text: copied, failed: false))
    }

    func testAGuestRestartAbandonsDropsInFlightButNotOneStillStaging() {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let queue = HvfFileDropQueue()
        let running = queue.begin(in: root)
        queue.staged(running.id, summary(files: 1, bytes: 10), sent: true)
        let staging = queue.begin(in: root)
        queue.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertEqual(queue.notice?.failed, true)
        XCTAssertEqual(queue.notice?.text.hasPrefix("Windows restarted"), true)
        queue.staged(staging.id, summary(files: 1, bytes: 10), sent: true)
        XCTAssertEqual(queue.notice?.text, "Starting the copy to Windows…")
    }

    /// Finder's Open With sends files to a VM that is still booting (HvfOpenFiles), and a drop can come
    /// before Guest Tools runs: the files are staged now and handed over at the agent's SERVICE start.
    func testADropWhileWindowsStartsIsSentWhenTheAgentServiceStarts() async throws {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let session = session(root: root)
        session.connectionState = .booting
        let source = root.appendingPathComponent("source/a.txt")
        try FileManager.default.createDirectory(at: source.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("a".utf8).write(to: source)

        await session.copyDroppedFiles([source], open: true)?.value
        XCTAssertEqual(session.fileDrops.notice, .init(text: "The copy starts when Windows is ready…", failed: false))
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.config.ctlFilePath))

        session.handle(.ready(host: "WIN", tMs: 1))
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.config.ctlFilePath), "the runtime reads it after SERVICE start")
        session.handle(.serviceStart(tMs: 2))
        let lines = try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8).split(separator: "\n")
        let drop = try XCTUnwrap(lines.first { $0.hasPrefix("BVDROP ") }).split(separator: " ").map(String.init)
        XCTAssertEqual(drop.count, 4)
        XCTAssertEqual(drop[3], "open")
        XCTAssertTrue(session.fileDrops.isPending(drop[1]))
        XCTAssertEqual(session.fileDrops.notice?.text, "Starting the copy to Windows…")
        session.handle(.serviceStart(tMs: 3))
        let again = try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
        XCTAssertEqual(again.components(separatedBy: "BVDROP ").count - 1, 1, "sent once")
    }

    #if canImport(AppKit)
    func testFramebufferRefusesTheDragWhenWindowsIsNotRunning() {
        let root = scratch()
        let session = session(root: root)
        let surface = FBLayerView(session: session)
        let pasteboard = NSPasteboard.withUniqueName()
        defer { pasteboard.releaseGlobally() }
        XCTAssertTrue(pasteboard.writeObjects([root.appendingPathComponent("example.txt") as NSURL]))

        XCTAssertFalse(surface.acceptFileDrop(from: pasteboard))
        XCTAssertEqual(session.fileDrops.notice, .init(text: HvfFileDropQueue.notRunningText, failed: true))
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.config.ctlFilePath))
        session.fileDrops.dismiss()
        XCTAssertNil(session.fileDrops.notice, "a failed notice closes")
    }
    #endif

    func testADropIsStagedNextToTheControlFileAndHandedToTheRuntime() async throws {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let session = session(root: root)
        session.serviceStarted = true
        session.connectionState = .connected(host: "WIN")
        let source = root.appendingPathComponent("source/CON.txt")
        try FileManager.default.createDirectory(at: source.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("con".utf8).write(to: source)

        await session.copyDroppedFiles([source])?.value

        let line = try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
        let fields = line.trimmingCharacters(in: .newlines).split(separator: " ").map(String.init)
        XCTAssertEqual(fields.count, 3)
        XCTAssertEqual(fields.first, "BVDROP")
        let staging = String(decoding: Data(base64Encoded: fields[2]) ?? Data(), as: UTF8.self)
        XCTAssertEqual(staging, root.appendingPathComponent("metadata/drops/\(fields[1])").path)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: staging + "/_CON.txt")), Data("con".utf8))
        XCTAssertTrue(session.events.contains { $0.displayText.contains("staged: files=1 folders=0 bytes=3 cloned=1 copied=0 skipped=0") })
        XCTAssertEqual(session.fileDrops.notice?.text, "Starting the copy to Windows…")
    }

    func testAStagingThatFinishesAfterTheVMStoppedIsDiscarded() async throws {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let session = session(root: root)
        session.serviceStarted = true
        session.connectionState = .connected(host: "WIN")
        let source = root.appendingPathComponent("source/a.txt")
        try FileManager.default.createDirectory(at: source.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("a".utf8).write(to: source)

        let staging = session.copyDroppedFiles([source])
        session.fileDrops.reset() // the VM stopped before staging finished
        await staging?.value

        XCTAssertFalse(FileManager.default.fileExists(atPath: session.config.ctlFilePath))
        let drops = root.appendingPathComponent("metadata/drops").path
        XCTAssertEqual((try? FileManager.default.contentsOfDirectory(atPath: drops)) ?? [], [])
    }
}

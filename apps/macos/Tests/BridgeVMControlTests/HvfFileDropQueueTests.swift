import Foundation
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
        XCTAssertEqual(queue.notice?.text, "끌어다 놓은 항목을 복사할 준비를 하고 있습니다…")
        queue.staged(job.id, summary(files: 2, bytes: 3000, skipped: 1), sent: true)
        queue.consume(.unknown("BVAGENT DROP start \(job.id) files=2 bytes=3000 t=10"))
        XCTAssertEqual(queue.notice?.text.hasPrefix("Windows로 복사 중: 파일 0/2개 · "), true)
        queue.consume(.unknown("BVAGENT DROP progress \(job.id) files=1/2 bytes=2000/3000 t=20\r"))
        XCTAssertEqual(queue.notice?.text.hasPrefix("Windows로 복사 중: 파일 1/2개 · "), true)
        queue.consume(.unknown("BVAGENT DROP done \(job.id) files=2 bytes=3000 ms=5 chunks=2 chunk_avg_us=9 t=30"))
        XCTAssertEqual(queue.notice?.failed, false)
        XCTAssertEqual(queue.notice?.text.hasPrefix("파일 2개("), true)
        XCTAssertEqual(queue.notice?.text.hasSuffix("바로가기와 특수 파일 1개는 건너뛰었습니다."), true)
        XCTAssertFalse(FileManager.default.fileExists(atPath: job.staging.path))
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
        XCTAssertEqual(queue.notice?.text.hasSuffix(" · 대기 1건"), true)
        queue.consume(.unknown("BVAGENT DROP start \(second.id) files=1 bytes=10 t=2 extra"))
        queue.consume(.unknown("BVAGENT DROP failed \(first.id) agent-too-old files=0/1 bytes=0/10 t=3"))
        XCTAssertEqual(queue.notice?.text.hasPrefix("Windows로 복사 중: 파일 0/1개"), true)
        queue.consume(.unknown("BVAGENT DROP failed \(second.id) guest-error bad-name files=0/1 bytes=0/10 t=4"))
        XCTAssertEqual(queue.notice, .init(text: HvfFileDropQueue.failureText("guest-error"), failed: true))
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
        XCTAssertEqual(queue.notice?.text.hasPrefix("Windows가 다시 시작되어"), true)
        queue.staged(staging.id, summary(files: 1, bytes: 10), sent: true)
        XCTAssertEqual(queue.notice?.text, "Windows로 복사를 시작하고 있습니다…")
    }

    func testADropBeforeTheAgentIsReadyIsRefusedWithANotice() {
        let root = scratch()
        defer { try? FileManager.default.removeItem(at: root) }
        let session = session(root: root)
        XCTAssertNil(session.copyDroppedFiles([root.appendingPathComponent("a.txt")]))
        XCTAssertEqual(session.fileDrops.notice, .init(text: HvfFileDropQueue.notReadyText, failed: true))
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.config.ctlFilePath))
    }

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
        XCTAssertEqual(session.fileDrops.notice?.text, "Windows로 복사를 시작하고 있습니다…")
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

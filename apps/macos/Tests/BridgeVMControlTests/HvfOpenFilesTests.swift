import Foundation
import XCTest
@testable import BridgeVMControl

/// Finder's Open With and a Dock drop (HvfOpenFiles): which VM gets the files, the `BVDROP ... open`
/// line it hands the runtime, and the one notice the runtime's done line becomes (file_drop.rs).
@MainActor
final class HvfOpenFilesTests: XCTestCase {
    private var roots: [URL] = []

    private func cleanUp() {
        for root in roots { try? FileManager.default.removeItem(at: root) }
        roots = []
    }

    private func session(running: Bool) -> HvfEngineSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("bvopen-\(UUID().uuidString)")
        roots.append(root)
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("metadata/hvf.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        if running {
            session.serviceStarted = true
            session.connectionState = .connected(host: "guest")
        }
        return session
    }

    private func file(_ name: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("bvopen-src-\(UUID().uuidString)")
        roots.append(dir)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(name)
        try Data("hello".utf8).write(to: url)
        return url
    }

    private func ctlLines(_ session: HvfEngineSession) -> [String] {
        ((try? String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)) ?? "")
            .split(separator: "\n").map(String.init)
    }

    /// Opens `urls` the way the app delegate does; what it presented and said.
    @discardableResult
    private func open(_ urls: [URL], sessions: [HvfEngineSession], front: [HvfEngineSession] = [])
        async -> (presented: [HvfEngineSession], told: [String]) {
        var presented: [HvfEngineSession] = []
        var told: [String] = []
        let work = HvfOpenFiles.open(urls, sessions: sessions, frontToBack: front,
                                     present: { presented.append($0) }, tell: { told.append($0) })
        await work?.value
        return (presented, told)
    }

    func testWithoutARunningWindowsOnlyOneNoticeAndNothingIsCopied() async throws {
        defer { cleanUp() }
        let stopped = session(running: false)
        let result = await open([try file("notes.txt")], sessions: [stopped], front: [stopped])
        XCTAssertEqual(result.told, ["Windows가 실행 중일 때 파일을 열 수 있습니다."])
        XCTAssertTrue(result.presented.isEmpty)
        XCTAssertTrue(ctlLines(stopped).isEmpty)
        XCTAssertNil(stopped.fileDrops.notice)
        let none = await open([try file("notes.txt")], sessions: [])
        XCTAssertEqual(none.told, [HvfOpenFiles.noWindowsText])
    }

    func testTheOnlyRunningVMCopiesTheFilesThenTheRuntimeOpensThem() async throws {
        defer { cleanUp() }
        let stopped = session(running: false)
        let running = session(running: true)
        let item = try file("notes.txt")
        // The stopped VM's window is in front; the only running VM still gets the files.
        let result = await open([item], sessions: [stopped, running], front: [stopped])
        XCTAssertTrue(result.told.isEmpty)
        XCTAssertTrue(result.presented.count == 1 && result.presented[0] === running)
        XCTAssertTrue(ctlLines(stopped).isEmpty)
        let lines = ctlLines(running)
        XCTAssertEqual(lines.count, 1)
        let fields = lines.first?.split(separator: " ").map(String.init) ?? []
        XCTAssertEqual(fields.count, 4)
        XCTAssertEqual(fields.first, "BVDROP")
        XCTAssertEqual(fields.last, "open")
        let id = fields.count == 4 ? fields[1] : ""
        let staging = fields.count == 4 ? Data(base64Encoded: fields[2]).flatMap { String(data: $0, encoding: .utf8) } : nil
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: staging ?? "/nonexistent"), ["notes.txt"])
        running.fileDrops.consume(.unknown("BVAGENT DROP start \(id) files=1 bytes=5 t=1"))
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(id) files=1 bytes=5 ms=3 chunks=1 chunk_avg_us=9 opened=1/1 t=2"))
        XCTAssertEqual(running.fileDrops.notice?.failed, false)
        XCTAssertEqual(running.fileDrops.notice?.text.hasSuffix("를 Windows 바탕 화면에 복사하고 열었습니다."), true)
        XCTAssertFalse(FileManager.default.fileExists(atPath: staging ?? "/"))
    }

    func testAnOldAgentOnlyCopiesAndSaysSoAndAFailedOpenSaysWhereTheFileIs() async throws {
        defer { cleanUp() }
        let running = session(running: true)
        await open([try file("a.txt")], sessions: [running])
        let id = ctlLines(running).last?.split(separator: " ").dropFirst().first.map(String.init) ?? ""
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(id) files=1 bytes=5 ms=3 chunks=1 chunk_avg_us=9 opened=old-agent t=2"))
        XCTAssertEqual(running.fileDrops.notice?.failed, true)
        XCTAssertEqual(running.fileDrops.notice?.text.hasPrefix("이 VM의 게스트 에이전트는 파일을 열 수 없어 파일 1개("), true)
        XCTAssertEqual(running.fileDrops.notice?.text.hasSuffix("를 Windows 바탕 화면에 복사만 했습니다."), true)

        await open([try file("b.txt"), try file("c.txt")], sessions: [running])
        let second = ctlLines(running).last?.split(separator: " ").dropFirst().first.map(String.init) ?? ""
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(second) files=2 bytes=10 ms=3 chunks=2 chunk_avg_us=9 opened=1/2 t=4"))
        XCTAssertEqual(running.fileDrops.notice?.failed, true)
        XCTAssertEqual(running.fileDrops.notice?.text.hasSuffix("복사했지만 1개는 열지 못했으니 바탕 화면에서 직접 여세요."), true)
    }

    func testWithTwoRunningVMsTheFrontWindowsVMGetsTheFilesElseOneNotice() async throws {
        defer { cleanUp() }
        let first = session(running: true)
        let second = session(running: true)
        let stopped = session(running: false)
        let result = await open([try file("x.txt")], sessions: [first, second, stopped], front: [stopped, second, first])
        XCTAssertTrue(result.told.isEmpty)
        XCTAssertTrue(result.presented.count == 1 && result.presented[0] === second)
        XCTAssertEqual(ctlLines(second).count, 1)
        XCTAssertTrue(ctlLines(first).isEmpty)

        let unclear = await open([try file("y.txt")], sessions: [first, second], front: [stopped])
        XCTAssertEqual(unclear.told, [HvfOpenFiles.whichWindowsText])
        XCTAssertTrue(unclear.presented.isEmpty)
        XCTAssertTrue(ctlLines(first).isEmpty)
        XCTAssertEqual(ctlLines(second).count, 1)
    }

    func testAWindowDropStaysAPlainCopy() async throws {
        defer { cleanUp() }
        let running = session(running: true)
        await running.copyDroppedFiles([try file("d.txt")])?.value
        let fields = ctlLines(running).first?.split(separator: " ") ?? []
        XCTAssertEqual(fields.count, 3)
        let id = fields.count == 3 ? String(fields[1]) : ""
        // A done line never makes a window drop say it opened anything.
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(id) files=1 bytes=5 ms=3 chunks=1 chunk_avg_us=9 opened=old-agent t=2"))
        XCTAssertEqual(running.fileDrops.notice?.failed, false)
        XCTAssertEqual(running.fileDrops.notice?.text.hasSuffix("를 Windows 바탕 화면에 복사했습니다."), true)
    }
}

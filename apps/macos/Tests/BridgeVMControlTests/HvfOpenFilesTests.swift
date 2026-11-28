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
        XCTAssertEqual(result.told, ["Start a Windows VM, then open the selection again."])
        XCTAssertTrue(result.presented.isEmpty)
        XCTAssertTrue(ctlLines(stopped).isEmpty)
        XCTAssertNil(stopped.fileDrops.notice)
        let none = await open([try file("notes.txt")], sessions: [])
        XCTAssertEqual(none.told, [HvfOpenFiles.noWindowsText])
    }

    /// Open With while BridgeVM was closed: the files arrived before the library had loaded (and before a
    /// VM set to start with BridgeVM had started) and were refused with "Start a Windows VM".
    func testFilesOpenedAtLaunchWaitForTheLibrary() {
        HvfOpenFilesAtLaunch.resetForTests()
        defer { HvfOpenFilesAtLaunch.resetForTests(); HvfOpenFilesAtLaunch.deliver = { _ in } }
        var delivered: [[URL]] = []
        HvfOpenFilesAtLaunch.deliver = { delivered.append($0) }
        let notes = URL(fileURLWithPath: "/tmp/notes.txt"), photo = URL(fileURLWithPath: "/tmp/photo.jpg")
        XCTAssertTrue(HvfOpenFilesAtLaunch.hold([notes]))
        XCTAssertTrue(HvfOpenFilesAtLaunch.hold([photo]))
        XCTAssertTrue(delivered.isEmpty, "nothing is refused before the library starts")
        HvfOpenFilesAtLaunch.libraryDidStart()
        XCTAssertEqual(delivered, [[notes, photo]])
        XCTAssertFalse(HvfOpenFilesAtLaunch.hold([notes]), "afterwards files go at once")
        HvfOpenFilesAtLaunch.libraryDidStart()
        XCTAssertEqual(delivered.count, 1)
    }

    func testTheOnlyRunningVMCopiesTwoFilesAndOpensTheirSelectedFolder() async throws {
        defer { cleanUp() }
        let stopped = session(running: false)
        let running = session(running: true)
        let first = try file("notes.txt")
        let folder = first.deletingLastPathComponent().appendingPathComponent("Project 자료")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try FileManager.default.moveItem(at: first, to: folder.appendingPathComponent("notes.txt"))
        try Data("hello".utf8).write(to: folder.appendingPathComponent("more.txt"))
        // The stopped VM's window is in front; the only running VM still gets the files.
        let result = await open([folder], sessions: [stopped, running], front: [stopped])
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
        let staged = URL(fileURLWithPath: try XCTUnwrap(staging))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: staged.path), ["Project 자료"])
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: staged.appendingPathComponent("Project 자료").path).sorted(), ["more.txt", "notes.txt"])
        running.fileDrops.consume(.unknown("BVAGENT DROP start \(id) files=2 bytes=10 t=1"))
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(id) files=2 bytes=10 ms=3 chunks=2 chunk_avg_us=9 opened=1/1 t=2"))
        XCTAssertEqual(running.fileDrops.notice?.failed, false)
        let size = ByteCountFormatter.string(fromByteCount: 10, countStyle: .file)
        XCTAssertEqual(running.fileDrops.notice?.text, "Copied to the Windows desktop (2 files, \(size)). Opened 1 selected item.")
        XCTAssertFalse(FileManager.default.fileExists(atPath: staging ?? "/"))
    }

    func testAnOldAgentOnlyCopiesAndSaysSoAndAFailedOpenSaysWhereTheFileIs() async throws {
        defer { cleanUp() }
        let running = session(running: true)
        await open([try file("a.txt")], sessions: [running])
        let id = ctlLines(running).last?.split(separator: " ").dropFirst().first.map(String.init) ?? ""
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(id) files=1 bytes=5 ms=3 chunks=1 chunk_avg_us=9 opened=old-agent t=2"))
        XCTAssertEqual(running.fileDrops.notice?.failed, true)
        XCTAssertEqual(running.fileDrops.notice?.text.hasPrefix("Copied to the Windows desktop (1 file, "), true)
        XCTAssertEqual(running.fileDrops.notice?.text.hasSuffix("This version of Guest Tools can’t open items automatically."), true)

        await open([try file("b.txt"), try file("c.txt")], sessions: [running])
        let second = ctlLines(running).last?.split(separator: " ").dropFirst().first.map(String.init) ?? ""
        running.fileDrops.consume(.unknown("BVAGENT DROP done \(second) files=2 bytes=10 ms=3 chunks=2 chunk_avg_us=9 opened=1/2 t=4"))
        XCTAssertEqual(running.fileDrops.notice?.failed, true)
        XCTAssertEqual(running.fileDrops.notice?.text.hasSuffix("Open 1 remaining item from the desktop."), true)
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
        let size = ByteCountFormatter.string(fromByteCount: 5, countStyle: .file)
        XCTAssertEqual(running.fileDrops.notice?.text, "Copied to the Windows desktop (1 file, \(size)).")
    }
}

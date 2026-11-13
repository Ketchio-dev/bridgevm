#if canImport(AppKit)
import CryptoKit
import Foundation
import XCTest
@testable import BridgeVMControl

/// "Mac으로 인쇄" (HvfGuestPrint.swift): the setting, the agent's PRINTWATCH, and the path from a probe
/// `BVPRINT job` line to the Mac's print panel.
@MainActor
final class HvfGuestPrintTests: XCTestCase {
    @MainActor
    private final class Presenter: HvfPrintPresenter {
        var shown: [URL] = []
        var pending: [@MainActor () -> Void] = []
        var succeeds = true
        func present(_ url: URL, done: @escaping @MainActor () -> Void) -> Bool {
            shown.append(url)
            guard succeeds else { return false }
            pending.append(done)
            return true
        }
        /// The user prints or cancels the open panel.
        func finish() {
            guard !pending.isEmpty else { return XCTFail("no print panel open") }
            pending.removeFirst()()
        }
    }

    private func session(_ fixture: HvfRuntimeSessionStoreFixture) throws -> (HvfEngineSession, Presenter, LibraryModel, VMConfig) {
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: HvfClipboardSyncProbe())
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let presenter = Presenter()
        session.printPresenter = presenter
        try FileManager.default.createDirectory(at: session.printDirectory, withIntermediateDirectories: true)
        return (session, presenter, library, saved)
    }

    private func pdf(_ text: String) -> Data { Data(("%PDF-1.7\n" + text + "\n%%EOF\n").utf8) }

    private func sha(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }

    /// A job file as the probe publishes it (`<probe start, Unix ms>-<n>.pdf`), and its line.
    private func job(_ session: HvfEngineSession, _ n: Int, _ data: Data, start: Int = 1_762_880_000_000) throws -> (URL, String) {
        let url = session.printDirectory.appendingPathComponent("\(start)-\(n).pdf")
        try data.write(to: url)
        return (url, "BVPRINT job bytes=\(data.count) sha256=\(sha(data)) path=\(url.path)")
    }

    private func exists(_ url: URL) -> Bool { FileManager.default.fileExists(atPath: url.path) }

    private func finishVerification(_ session: HvfEngineSession) async {
        while let pending = session.printVerificationTask { await pending.value }
    }

    private final class VerificationGate: @unchecked Sendable {
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let completed = DispatchSemaphore(value: 0)
        func verify(_ job: HvfGuestPrint.Job) -> Bool {
            entered.signal()
            let responsive = release.wait(timeout: .now() + 2) == .success
            completed.signal()
            return responsive
        }
    }

    func testVerificationYieldsMainActorAndStopDiscardsPendingJob() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        let gate = VerificationGate()
        session.printVerifier = gate.verify
        let (_, line) = try job(session, 1, pdf("pending"))
        session.handle(.unknown(line))
        // This operation must run while the verifier is still pending.
        let entered = await Task.detached { gate.entered.wait(timeout: .now() + 3) == .success }.value
        XCTAssertTrue(entered)
        XCTAssertEqual(gate.completed.wait(timeout: .now()), .timedOut, "verification must not block the main actor")
        let pending = session.printVerificationTask
        session.markStopped()
        gate.release.signal()
        _ = await Task.detached { gate.completed.wait(timeout: .now() + 3) }.value
        await pending?.value
        XCTAssertTrue(presenter.shown.isEmpty, "a stopped session must not present its pending job")
    }

    func testPendingVerificationKeepsIdentityAndDoesNotPresentDuplicate() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        let gate = VerificationGate()
        session.printVerifier = gate.verify
        let (first, line) = try job(session, 1, pdf("first"))
        let (second, nextLine) = try job(session, 2, pdf("second"))
        session.handle(.unknown(line))
        let entered = await Task.detached { gate.entered.wait(timeout: .now() + 3) == .success }.value
        XCTAssertTrue(entered)
        session.handle(.unknown(line))
        session.handle(.unknown(nextLine))
        session.handle(.unknown(nextLine))
        XCTAssertEqual(session.printQueue.map(\.url), [second])
        gate.release.signal()
        await finishVerification(session)
        XCTAssertEqual(presenter.shown, [first])
        session.printVerifier = HvfGuestPrint.verified
        presenter.finish()
        await finishVerification(session)
        XCTAssertEqual(presenter.shown, [first, second])
        presenter.finish()
        XCTAssertTrue(session.printQueue.isEmpty)
    }

    func testPresentedJobFinishesAfterStopOrResetAndIsNotOfferedAgain() async throws {
        for stop in [true, false] {
            let fixture = try HvfRuntimeSessionStoreFixture()
            defer { fixture.clean() }
            let (session, presenter, _, _) = try session(fixture)
            let (url, line) = try job(session, 1, pdf("open panel"))
            session.handle(.unknown(line))
            await finishVerification(session)
            XCTAssertEqual(presenter.shown, [url])
            if stop { session.markStopped() } else { session.resetObservedRuntimeState(clearEvents: true) }
            presenter.finish()
            XCTAssertFalse(exists(url), "an already-presented job must be deleted when its panel finishes after stop/reset")
            session.connectionState = .connected(host: "guest")
            session.handle(.serviceStart(tMs: 1))
            await finishVerification(session)
            XCTAssertEqual(presenter.shown, [url], "completed job must not be offered by leftovers again")
            XCTAssertTrue(session.printQueue.isEmpty)
        }
    }

    func testOldPanelRemainsExclusiveAcrossStopAndNewRun() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        let (old, line) = try job(session, 1, pdf("old panel"))
        session.handle(.unknown(line))
        await finishVerification(session)
        session.markStopped()
        session.resetObservedRuntimeState(clearEvents: true)
        let (next, nextLine) = try job(session, 2, pdf("new run"))
        session.connectionState = .connected(host: "guest")
        session.handle(.serviceStart(tMs: 1))
        session.handle(.unknown(nextLine))
        await finishVerification(session)
        XCTAssertEqual(presenter.shown, [old], "old panel must retain the single presentation slot across runs")
        XCTAssertEqual(session.printShowing, old)
        presenter.finish()
        await finishVerification(session)
        XCTAssertFalse(exists(old))
        XCTAssertEqual(presenter.shown, [old, next], "new job must be presented once, only after old panel ends")
        presenter.finish()
        XCTAssertFalse(exists(next))
        XCTAssertTrue(session.printQueue.isEmpty)
    }

    func testPrintWatchGoesRawNeverAsAShellCommand() {
        XCTAssertTrue(HvfGuestCommand.relayProtocolVerbs.contains(HvfGuestPrint.watchCommand))
        XCTAssertEqual(try HvfGuestCommand.normalize(HvfGuestPrint.offCommand).get(), "PRINTWATCH off")
    }

    func testOnlyAJobFileDirectlyInTheProbesPrintFolderCounts() {
        let dir = URL(fileURLWithPath: "/Users/u/Library/Application Support/BridgeVM/vms/w/bundle/logs/hvf/print")
        let hex = String(repeating: "ab", count: 32)
        let good = "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/1762880000000-3.pdf"
        XCTAssertEqual(HvfGuestPrint.job(from: good, directory: dir),
                       HvfGuestPrint.Job(url: dir.appendingPathComponent("1762880000000-3.pdf"), bytes: 12, sha256: hex))
        XCTAssertNotNil(HvfGuestPrint.job(from: good + "\r", directory: dir))
        for bad in [
            "BVPRINT job bytes=12 sha256=\(hex) path=/Users/u/Documents/1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/../../../../../../../Documents/1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/../print/../../1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/sub/1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/3.pdf",  // the earlier probe's name form
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/report.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/1762880000000-3.pdf.part",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/1762880000000-.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/1-2-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/1762880000000-3a.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=\(dir.path)/.pdf",
            "BVPRINT job bytes=12 sha256=\(hex) path=print/1762880000000-3.pdf",
            "BVPRINT job bytes=0 sha256=\(hex) path=\(dir.path)/1762880000000-3.pdf",
            "BVPRINT job bytes=\(HvfGuestPrint.maxJobBytes + 1) sha256=\(hex) path=\(dir.path)/1762880000000-3.pdf",
            "BVPRINT job bytes=x sha256=\(hex) path=\(dir.path)/1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex.uppercased()) path=\(dir.path)/1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=abc path=\(dir.path)/1762880000000-3.pdf",
            "BVPRINT job bytes=12 extra=1 sha256=\(hex) path=\(dir.path)/1762880000000-3.pdf",
            "BVPRINT job bytes=12 sha256=\(hex)",
            "BVPRINT refused reason=too-large bytes=999",
        ] {
            XCTAssertNil(HvfGuestPrint.job(from: bad, directory: dir), bad)
        }
    }

    /// Jobs an earlier probe published that never reached a print panel (the app quit or the VM stopped first;
    /// a restarted probe keeps them) open at the next agent start, oldest first, each once.
    func testJobsLeftByAnEarlierProbeOpenAtTheAgentStart() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        let control = URL(fileURLWithPath: session.config.ctlFilePath)
        try FileManager.default.createDirectory(at: control.deletingLastPathComponent(), withIntermediateDirectories: true)
        let (later, _) = try job(session, 1, pdf("later"), start: 1_762_890_000_000)
        let (tenth, tenthLine) = try job(session, 10, pdf("tenth"))
        let (second, _) = try job(session, 2, pdf("second"))
        let others = ["1762890000000-2.pdf.part", "3.pdf", "notes.txt"].map { session.printDirectory.appendingPathComponent($0) }
        for url in others { try pdf("other").write(to: url) }
        func agentStarts() { session.connectionState = .connected(host: "guest"); session.handle(.serviceStart(tMs: 1)) }
        agentStarts()
        await finishVerification(session)
        XCTAssertEqual(presenter.shown, [second], "one panel at a time, oldest first")
        session.handle(.unknown(tenthLine))  // its line read again: still one panel for it
        agentStarts()                        // the next boot: the same jobs, not again
        presenter.finish()
        await finishVerification(session)
        presenter.finish()
        await finishVerification(session)
        presenter.finish()
        XCTAssertEqual(presenter.shown, [second, tenth, later])
        XCTAssertTrue(presenter.pending.isEmpty)
        XCTAssertFalse(exists(second) || exists(tenth) || exists(later), "each deleted after its panel")
        XCTAssertTrue(others.allSatisfy(exists), "nothing else is opened or deleted")
        XCTAssertNil(session.printNotice.shown)
    }

    /// Each job opens one print panel, the next only after it, and its file goes once its panel is over.
    func testEachJobOpensOnePrintPanelInTurnAndItsFileGoesAfter() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        XCTAssertTrue(session.printToMacWanted, "on by default")
        let (first, firstLine) = try job(session, 1, pdf("one"))
        let (second, secondLine) = try job(session, 2, pdf("two"))
        session.handle(.unknown(firstLine))
        session.handle(.unknown(secondLine))
        session.handle(.unknown(secondLine))  // the same line again: still one panel for it
        await finishVerification(session)
        XCTAssertEqual(presenter.shown, [first], "one panel at a time")
        XCTAssertTrue(exists(first) && exists(second))
        presenter.finish()
        XCTAssertFalse(exists(first), "deleted after its panel")
        await finishVerification(session)
        XCTAssertEqual(presenter.shown, [first, second])
        presenter.finish()
        XCTAssertFalse(exists(second))
        XCTAssertTrue(presenter.pending.isEmpty)
        // run.log read again (a reattached session): the jobs are gone, nothing opens and nothing is said.
        session.handle(.unknown(firstLine))
        XCTAssertEqual(presenter.shown, [first, second])
        XCTAssertNil(session.printNotice.shown)
    }

    /// A file that is not the line's job (another size or digest, a link, not a PDF) never reaches the panel.
    func testAFileThatIsNotTheLinesJobIsNeverShown() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        let secret = fixture.root.appendingPathComponent("secret.pdf")
        try pdf("secret").write(to: secret)
        let (changed, changedLine) = try job(session, 1, pdf("one"))
        try pdf("One").write(to: changed)
        let (_, sizeLine) = try job(session, 2, pdf("two"))
        let link = session.printDirectory.appendingPathComponent("1762880000000-3.pdf")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: secret)
        let linkLine = "BVPRINT job bytes=\(pdf("secret").count) sha256=\(sha(pdf("secret"))) path=\(link.path)"
        let (_, zipLine) = try job(session, 4, Data("PK\u{3}\u{4} not a pdf".utf8))
        for line in [changedLine, sizeLine.replacingOccurrences(of: "bytes=\(pdf("two").count)", with: "bytes=3"), linkLine, zipLine] {
            session.handle(.unknown(line))
        }
        await finishVerification(session)
        XCTAssertTrue(presenter.shown.isEmpty)
        XCTAssertEqual(session.printNotice.shown, .notReceived)
        XCTAssertTrue(exists(secret), "a link's target is never deleted")
        XCTAssertFalse(FileManager.default.fileExists(atPath: changed.path))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: session.printDirectory.path), [], "none kept")
    }

    /// A printer Windows could not make, a job the probe refused, a job the panel cannot open: one Korean notice
    /// each, and the VM goes on.
    func testFailuresAreOneKoreanNoticeEach() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, _, _) = try session(fixture)
        session.handle(.commandOutput(label: "PRINTWATCH", body: "BVPRINT printer=ready"))
        XCTAssertNil(session.printNotice.shown)
        session.handle(.commandOutput(label: "PRINTWATCH", body: "BVPRINT printer=failed detail=The spooler service is not reachable."))
        XCTAssertEqual(session.printNotice.shown, .printerFailed)
        let text = try XCTUnwrap(session.printNotice.text)
        XCTAssertTrue(text.contains("Mac 프린터를 만들지 못했으니") && text.hasSuffix("다시 시작하세요."), text)
        XCTAssertEqual(text.filter { $0 == "." }.count, 1, "one sentence")
        session.printNotice.show(nil)
        session.handle(.commandOutput(label: "PRINTWATCH", body: "BVPRINT printer=failed detail=again"))
        XCTAssertNil(session.printNotice.shown, "once per app session")

        session.handle(.unknown("BVPRINT refused reason=too-large bytes=300000000"))
        XCTAssertEqual(session.printNotice.shown, .tooLarge)
        session.handle(.unknown("BVPRINT refused reason=short bytes=3000"))
        XCTAssertEqual(session.printNotice.shown, .notReceived)

        presenter.succeeds = false
        let (url, line) = try job(session, 1, pdf("one"))
        session.handle(.unknown(line))
        await finishVerification(session)
        XCTAssertEqual(session.printNotice.shown, .notOpened)
        XCTAssertFalse(exists(url), "a job that cannot be shown is not kept")
        presenter.succeeds = true
        let (next, nextLine) = try job(session, 2, pdf("two"))
        session.handle(.unknown(nextLine))
        await finishVerification(session)
        XCTAssertEqual(presenter.shown.last, next, "the next job still opens")
        presenter.finish()
        XCTAssertNil(session.printNotice.shown, "a printed job clears the notice")
        for kind in [HvfGuestPrintNotice.Kind.tooLarge, .notReceived, .notOpened] {
            session.printNotice.show(kind)
            XCTAssertEqual(session.printNotice.text?.filter { $0 == "." }.count, 1, "\(kind): one sentence")
        }
    }

    /// On (the default): every agent start sends PRINTWATCH. Turned off: PRINTWATCH off once per app session, and
    /// no job opens. On again: PRINTWATCH.
    func testAgentStartWatchesByDefaultAndRemovesThePrinterOnceWhenTurnedOff() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (session, presenter, library, saved) = try session(fixture)
        let control = URL(fileURLWithPath: session.config.ctlFilePath)
        try FileManager.default.createDirectory(at: control.deletingLastPathComponent(), withIntermediateDirectories: true)
        func sent() -> [String] {
            ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n").map(String.init)
                .filter { $0.hasPrefix("PRINTWATCH") }
        }
        func agentStarts() { session.connectionState = .connected(host: "guest"); session.handle(.serviceStart(tMs: 1)) }
        func change(_ enabled: Bool) {
            session.connectionState = .stopped
            XCTAssertTrue(session.applyPrintToggle(enabled) == (enabled, nil))
        }
        agentStarts()
        agentStarts()
        XCTAssertEqual(sent(), ["PRINTWATCH", "PRINTWATCH"], "never set is on, every boot")
        change(true)
        XCTAssertNil(library.latestConfiguration(for: saved).printToMac, "on while never set stays absent")
        change(false)
        XCTAssertEqual(library.latestConfiguration(for: saved).printToMac, false)
        agentStarts()
        agentStarts()
        XCTAssertEqual(sent(), ["PRINTWATCH", "PRINTWATCH", "PRINTWATCH off"], "removed once per app session")
        let (url, line) = try job(session, 1, pdf("off"))
        session.handle(.unknown(line))
        session.handle(.commandOutput(label: "PRINTWATCH", body: "BVPRINT printer=failed detail=x"))
        XCTAssertTrue(presenter.shown.isEmpty && session.printNotice.shown == nil, "off: nothing opens, nothing is said")
        XCTAssertTrue(exists(url))
        change(true)
        XCTAssertEqual(library.latestConfiguration(for: saved).printToMac, true)
        agentStarts()
        XCTAssertEqual(sent().last, "PRINTWATCH")
        session.connectionState = .connected(host: "guest")
        XCTAssertEqual(session.applyPrintToggle(false).message, "VM을 완전히 중지한 뒤 인쇄 설정을 변경하세요.")
        XCTAssertNil(library.operationError)
    }
}
#endif

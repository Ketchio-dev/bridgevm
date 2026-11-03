import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDisplayDriverInstallScriptTests: XCTestCase {
    private typealias Install = HvfDisplayDriverInstall

    func testCommandIsOneAgentLineCarryingTheScript() throws {
        let command = Install.command
        XCTAssertEqual(try HvfGuestCommand.normalizeShell(command).get(), command)
        XCTAssertTrue(command.utf8.count <= HvfClipboardPaste.maximumCommandCharacters, "\(command.utf8.count)")
        let prefix = "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand "
        XCTAssertTrue(command.hasPrefix(prefix))
        let data = try XCTUnwrap(Data(base64Encoded: String(command.dropFirst(prefix.count))))
        XCTAssertEqual(String(data: data, encoding: .utf16LittleEndian), Install.script)
    }

    func testScriptDownloadsThePinAndRefusesAnyOtherBytes() {
        let script = Install.script
        XCTAssertTrue(script.contains("Invoke-WebRequest -UseBasicParsing -UserAgent 'BridgeVM' -Uri 'https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/upstream-virtio/attestation-virtio-win-prewhql-0.1-299.zip' -OutFile $zip"))
        XCTAssertTrue(script.contains("[Net.SecurityProtocolType]::Tls12"))
        XCTAssertTrue(script.contains("if ($length -ne 94849704) { throw"))
        XCTAssertTrue(script.contains("(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash"))
        XCTAssertTrue(script.contains("if ($hash -ne 'ff393c5338c35d1606a0108d2d818a29be48de373be7aac95b632aa5a46aead0') { throw"))
        // Both checks run before anything is extracted or installed.
        let checks = script.range(of: "if ($hash -ne")!.lowerBound
        XCTAssertLessThan(checks, script.range(of: "OpenRead")!.lowerBound)
        XCTAssertLessThan(checks, script.range(of: "pnputil.exe")!.lowerBound)
    }

    func testScriptExtractsOnlyTheThreeDriverFilesAndInstallsThem() {
        let script = Install.script
        XCTAssertTrue(script.contains("foreach ($name in 'viogpudo.inf', 'viogpudo.cat', 'viogpudo.sys') {"))
        XCTAssertTrue(script.contains("$archive.GetEntry('Win11/ARM64/' + $name)"))
        XCTAssertEqual(script.components(separatedBy: "ExtractToFile(").count - 1, 1)
        XCTAssertFalse(script.contains("ExtractToDirectory"))
        XCTAssertFalse(script.contains("Expand-Archive"))
        XCTAssertTrue(script.contains("pnputil.exe /add-driver (Join-Path $work 'viogpudo.inf') /install"))
        XCTAssertTrue(script.contains("Remove-Item -LiteralPath $zip -Force"))
    }

    func testScriptPrintsExactlyOneResultLine() {
        let script = Install.script
        XCTAssertEqual(script.components(separatedBy: "Write-Output").count - 1, 1)
        XCTAssertTrue(script.hasSuffix("Write-Output ('BVDRIVER result=' + $result + ' detail=' + (([string]$detail -replace '\\s+', ' ').Trim()))"))
        for result in ["'installed'", "'staged'", "'failed'"] {
            XCTAssertTrue(script.contains("$result = \(result)"), result)
        }
        XCTAssertTrue(script.contains("$_.InstanceId -like 'PCI\\VEN_1AF4&DEV_1050*'"))
    }
}

final class HvfDisplayDriverInstallParseTests: XCTestCase {
    private func parse(_ output: String, code: Int32 = 0) -> HvfDisplayDriverInstall.Outcome {
        HvfDisplayDriverInstall.parse(output: output, code: code)
    }

    func testParsesTheGuestResultLines() {
        let installed = parse("Microsoft PnP Utility\r\n\r\nBVDRIVER result=installed detail=Red Hat VirtIO GPU DOD controller status=OK\r\n")
        XCTAssertEqual(installed, .init(status: .installed, detail: "Red Hat VirtIO GPU DOD controller status=OK"))
        XCTAssertTrue(installed.leavesDriverInGuest)

        let staged = parse("BVDRIVER result=staged detail=no 1AF4:1050 device yet; Windows binds the driver on the next boot with one")
        XCTAssertEqual(staged.status, .staged)
        XCTAssertTrue(staged.leavesDriverInGuest)

        let failed = parse("BVDRIVER result=failed detail=download SHA-256 00AA does not match the pinned ff39", code: 0)
        XCTAssertEqual(failed, .init(status: .failed, detail: "download SHA-256 00AA does not match the pinned ff39"))
        XCTAssertFalse(failed.leavesDriverInGuest)
        XCTAssertTrue(failed.message.contains("download SHA-256 00AA"))
    }

    func testMissingOrUnknownResultLineIsAFailure() {
        let silent = parse("", code: 0)
        XCTAssertEqual(silent.status, .failed)
        XCTAssertTrue(silent.detail.contains("BVDRIVER"))

        let refused = parse(HvfAgentServiceGate.serviceNotStartedMessage, code: -1)
        XCTAssertEqual(refused.status, .failed)
        XCTAssertTrue(refused.detail.contains("exit -1"))
        XCTAssertTrue(refused.detail.contains(HvfAgentServiceGate.serviceNotStartedMessage))

        XCTAssertEqual(parse("BVDRIVER result=maybe detail=x").status, .failed)
    }
}

@MainActor
final class HvfDisplayDriverInstallSessionTests: XCTestCase {
    func testActionIsDisabledWhileTheAgentIsNotReady() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let session = fixture.session
        XCTAssertFalse(HvfDisplayDriverInstall.agentReady(session))
        session.connectionState = .booting
        session.serviceStarted = true
        XCTAssertFalse(HvfDisplayDriverInstall.agentReady(session))
        session.connectionState = .connected(host: "guest")
        session.serviceStarted = false
        XCTAssertFalse(HvfDisplayDriverInstall.agentReady(session))
        session.serviceStarted = true
        XCTAssertTrue(HvfDisplayDriverInstall.agentReady(session))
        session.connectionState = .stopping
        XCTAssertFalse(HvfDisplayDriverInstall.agentReady(session))
    }

    func testRunnerRefusesWithoutWritingWhenTheAgentIsNotReady() async throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let reply = await HvfDisplayDriverInstall.runInGuest(HvfDisplayDriverInstall.command, session: fixture.session, timeout: 1)
        XCTAssertEqual(reply.code, -1)
        XCTAssertEqual(reply.output, HvfAgentServiceGate.serviceNotStartedMessage)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.control.path))
    }

    func testInstallSendsTheCommandThroughTheAgentAndReportsTheReply() async throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        fixture.session.serviceStarted = true
        fixture.session.connectionState = .connected(host: "guest")
        try "BVAGENT SERVICE start t=1\n".write(to: fixture.log, atomically: true, encoding: .utf8)
        let command = HvfDisplayDriverInstall.command
        var installed: [HvfDisplayDriverInstall.Outcome] = []
        var outcome: HvfDisplayDriverInstall.Outcome?
        HvfDisplayDriverInstall.start(
            runner: { await HvfDisplayDriverInstall.runInGuest($0, session: fixture.session, timeout: 20) },
            onInstalled: { installed.append($0) }
        ) { outcome = $0 }

        while (try? String(contentsOf: fixture.control, encoding: .utf8)) != command + "\n" {
            try await Task.sleep(nanoseconds: 20_000_000)
        }
        let reply = "BVAGENT CMD \(command) exit=0\nBVDRIVER result=staged detail=no 1AF4:1050 device yet\nBVAGENT END \(command)\n"
        let handle = try FileHandle(forWritingTo: fixture.log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(reply.utf8))
        try handle.close()

        let deadline = Date().addingTimeInterval(10)
        while outcome == nil, Date() < deadline { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertEqual(outcome, .init(status: .staged, detail: "no 1AF4:1050 device yet"))
        XCTAssertEqual(installed, outcome.map { [$0] } ?? [])
    }

    func testFailedOutcomeDoesNotReachOnInstalled() async {
        var installed = 0
        var sent: [String] = []
        let outcome = await withCheckedContinuation { (done: CheckedContinuation<HvfDisplayDriverInstall.Outcome, Never>) in
            HvfDisplayDriverInstall.start(
                runner: { sent.append($0); return ("BVDRIVER result=failed detail=pnputil exit 5", 0) },
                onInstalled: { _ in installed += 1 }
            ) { done.resume(returning: $0) }
        }
        XCTAssertEqual(sent, [HvfDisplayDriverInstall.command])
        XCTAssertEqual(outcome.status, .failed)
        XCTAssertEqual(installed, 0)
    }

    private struct Fixture {
        let root: URL
        let control: URL
        let log: URL
        let session: HvfEngineSession

        @MainActor init() throws {
            root = FileManager.default.temporaryDirectory.appendingPathComponent("display-driver-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            control = root.appendingPathComponent("agent.ctl")
            log = root.appendingPathComponent("run.log")
            let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
                evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false,
                shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
                nvmeBufferedIO: false, ctlFilePath: control.path)
            session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}

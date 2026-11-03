import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDisplayDriverInstallScriptTests: XCTestCase {
    private typealias Install = HvfDisplayDriverInstall
    private let powershell = "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand "

    private func decodeUTF16(_ base64: Substring) -> String? {
        Data(base64Encoded: String(base64)).flatMap { String(data: $0, encoding: .utf16LittleEndian) }
    }

    /// The launch line writes install.ps1 and runs its Launch mode; the poll line runs Poll.
    func testLaunchAndPollAreEachOneAgentLineThatFitsCmd() throws {
        for command in [Install.launchCommand, Install.pollCommand] {
            XCTAssertEqual(try HvfGuestCommand.normalizeShell(command).get(), command)
            XCTAssertTrue(command.utf8.count <= HvfClipboardPaste.maximumCommandCharacters, "\(command.utf8.count)")
            // Base64 and the fixed wrapper only: nothing cmd.exe would expand, quote or redirect.
            XCTAssertFalse(command.contains(where: { "\"%^|<>".contains($0) }))
        }
        let parts = Install.launchCommand.components(separatedBy: "&& ")
        XCTAssertEqual(parts.count, 2)
        let set = "set BVDRIVER_INSTALLER="
        XCTAssertTrue(parts[0].hasPrefix(set))
        let script = try XCTUnwrap(Data(base64Encoded: String(parts[0].dropFirst(set.count))))
        XCTAssertEqual(String(data: script, encoding: .utf8), Install.script)
        XCTAssertTrue(parts[1].hasPrefix(powershell))
        XCTAssertEqual(decodeUTF16(parts[1].dropFirst(powershell.count)), """
            $ErrorActionPreference = 'Stop'
            $file = Join-Path $env:ProgramData 'BridgeVM\\display-driver\\install.ps1'
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $file) | Out-Null
            [IO.File]::WriteAllBytes($file, [Convert]::FromBase64String($env:BVDRIVER_INSTALLER))
            & $file -Mode Launch
            """)
        XCTAssertTrue(Install.pollCommand.hasPrefix(powershell))
        XCTAssertEqual(decodeUTF16(Install.pollCommand.dropFirst(powershell.count)),
                       "& (Join-Path $env:ProgramData 'BridgeVM\\display-driver\\install.ps1') -Mode Poll")
        // Windows PowerShell 5.1 reads a BOM-less script in the ANSI code page.
        XCTAssertTrue(Install.script.unicodeScalars.allSatisfy(\.isASCII))
    }

    func testLaunchStartsTheInstallerDetachedUnlessOneIsRunning() throws {
        let script = Install.script
        XCTAssertTrue(script.hasPrefix("param([ValidateSet('Launch', 'Poll', 'Run')] [string]$Mode = 'Run')\n"))
        XCTAssertTrue(script.contains("$dir = Join-Path $env:ProgramData 'BridgeVM\\display-driver'"))
        // The lock's PID counts only while it is still this script's Run mode; a stale lock is ignored.
        XCTAssertTrue(script.contains("if ($null -ne $process -and $process.CommandLine -like '*\\BridgeVM\\display-driver\\install.ps1*-Mode Run*') {\n      'BVDRIVER running pid=' + $id\n    } elseif"))
        XCTAssertTrue(script.contains("$line = 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"' + $PSCommandPath + '\" -Mode Run'"))
        XCTAssertTrue(script.contains("Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $line }"))
        XCTAssertFalse(script.contains("Start-Process")) // it would inherit the agent's output pipe
        let order = ["if ($null -ne $process", "Remove-Item -LiteralPath $resultFile -Force", "Invoke-CimMethod",
                     "[IO.File]::WriteAllText($lock, [string]$started.ProcessId)", "'BVDRIVER started pid='", "exit\n}\n$zip ="]
        let positions = try order.map { try XCTUnwrap(script.range(of: $0), $0).lowerBound }
        XCTAssertEqual(positions, positions.sorted())
    }

    func testPollPrintsTheResultFileElseRunningElseAFailure() {
        let script = Install.script
        XCTAssertTrue(script.contains("if ($Mode -eq 'Poll' -and (Test-Path -LiteralPath $resultFile)) { [IO.File]::ReadAllText($resultFile); exit }"))
        XCTAssertTrue(script.contains("} elseif ($Mode -eq 'Poll') {\n      Format-Result 'failed' 'the installer stopped without a result"))
    }

    func testRunResumesTheKeptPartialFileWithAFreshCurlRunPerAttempt() throws {
        let script = Install.script
        XCTAssertTrue(script.contains("$zip = Join-Path $dir 'virtio-win.zip'"))
        XCTAssertTrue(script.contains("$curl = Join-Path $env:SystemRoot 'System32\\curl.exe'"))
        XCTAssertTrue(script.contains("while ((Get-ZipLength) -lt 94849704) {\n    $before = Get-ZipLength\n    & $curl -sS -A BridgeVM --fail --location --speed-limit 20000 --speed-time 20 --connect-timeout 30 -C '-' --stderr $log -o $zip 'https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/upstream-virtio/attestation-virtio-win-prewhql-0.1-299.zip'\n"))
        XCTAssertFalse(script.contains("--retry")) // curl's own retry discards the failed attempt's bytes
        XCTAssertTrue(script.contains("if ((Get-ZipLength) -le $before) { $misses++ }"))
        XCTAssertTrue(script.contains("if ($misses -ge 10) { throw ('curl.exe exit '"))
        XCTAssertFalse(script.contains("Invoke-WebRequest"))
        // The partial file outlives failed attempts: only a pin mismatch or a finished install deletes it.
        XCTAssertEqual(script.components(separatedBy: "Remove-Item -LiteralPath $zip ").count - 1, 3)
        XCTAssertFalse(script.contains("Remove-Item -LiteralPath $dir"))
        let curl = try XCTUnwrap(script.range(of: "& $curl")).lowerBound
        XCTAssertLessThan(curl, try XCTUnwrap(script.range(of: "Remove-Item -LiteralPath $zip ")).lowerBound)
    }

    func testRunChecksBothPinsBeforeExtractingOrInstalling() throws {
        let script = Install.script
        XCTAssertTrue(script.contains("if ($length -ne 94849704) {"))
        XCTAssertTrue(script.contains("(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash"))
        XCTAssertTrue(script.contains("if ($hash -ne 'ff393c5338c35d1606a0108d2d818a29be48de373be7aac95b632aa5a46aead0') {"))
        let length = try XCTUnwrap(script.range(of: "if ($length -ne")).lowerBound
        let hash = try XCTUnwrap(script.range(of: "if ($hash -ne")).lowerBound
        XCTAssertLessThan(try XCTUnwrap(script.range(of: "while ((Get-ZipLength)")).lowerBound, length)
        XCTAssertLessThan(length, hash)
        XCTAssertLessThan(hash, try XCTUnwrap(script.range(of: "OpenRead")).lowerBound)
        XCTAssertLessThan(hash, try XCTUnwrap(script.range(of: "pnputil.exe")).lowerBound)
    }

    func testRunExtractsOnlyTheThreeDriverFilesAndInstallsThem() {
        let script = Install.script
        XCTAssertTrue(script.contains("foreach ($name in 'viogpudo.inf', 'viogpudo.cat', 'viogpudo.sys') {"))
        XCTAssertTrue(script.contains("$archive.GetEntry('Win11/ARM64/' + $name)"))
        XCTAssertEqual(script.components(separatedBy: "ExtractToFile(").count - 1, 1)
        XCTAssertFalse(script.contains("ExtractToDirectory"))
        XCTAssertFalse(script.contains("Expand-Archive"))
        XCTAssertTrue(script.contains("pnputil.exe /add-driver (Join-Path $work 'viogpudo.inf') /install"))
    }

    func testRunWritesOneWholeResultLineThenDropsTheLock() {
        let script = Install.script
        XCTAssertEqual(script.components(separatedBy: "'BVDRIVER result='").count - 1, 1)
        XCTAssertTrue(script.hasSuffix("""
            $temp = Join-Path $dir 'result.tmp'
            [IO.File]::WriteAllText($temp, (Format-Result $result $detail))
            [IO.File]::Move($temp, $resultFile)
            Remove-Item -LiteralPath $lock -Force -ErrorAction SilentlyContinue
            """))
        for result in ["'installed'", "'staged'", "'failed'"] {
            XCTAssertTrue(script.contains("$result = \(result)"), result)
        }
        XCTAssertTrue(script.contains("$_.InstanceId -like 'PCI\\VEN_1AF4&DEV_1050*'"))
    }
}

final class HvfDisplayDriverInstallProgressTests: XCTestCase {
    private func progress(_ output: String, code: Int32 = 0) -> HvfDisplayDriverInstall.Progress {
        HvfDisplayDriverInstall.progress(output: output, code: code)
    }

    private func outcome(_ output: String, code: Int32 = 0) -> HvfDisplayDriverInstall.Outcome? {
        guard case let .finished(outcome) = progress(output, code: code) else { return nil }
        return outcome
    }

    func testStartedOrRunningMeansPollAgain() {
        XCTAssertEqual(progress("BVDRIVER started pid=4242\r\n"), .running) // just launched, no result yet
        XCTAssertEqual(progress("BVDRIVER running pid=4242"), .running) // a later poll, or a second click
    }

    func testParsesTheResultLines() {
        let installed = outcome("BVDRIVER result=installed detail=Red Hat VirtIO GPU DOD controller status=OK\r\n")
        XCTAssertEqual(installed, .init(status: .installed, detail: "Red Hat VirtIO GPU DOD controller status=OK"))
        XCTAssertEqual(installed?.leavesDriverInGuest, true)

        let staged = outcome("BVDRIVER result=staged detail=no 1AF4:1050 device yet; it binds on the next boot with one")
        XCTAssertEqual(staged?.status, .staged)
        XCTAssertEqual(staged?.leavesDriverInGuest, true)

        let failed = outcome("BVDRIVER result=failed detail=download SHA-256 00AA does not match the pinned ff39")
        XCTAssertEqual(failed, .init(status: .failed, detail: "download SHA-256 00AA does not match the pinned ff39"))
        XCTAssertEqual(failed?.leavesDriverInGuest, false)
        XCTAssertEqual(failed?.message.contains("download SHA-256 00AA"), true)
    }

    func testMissingOrUnknownLineIsAFailure() {
        let silent = outcome("", code: 0)
        XCTAssertEqual(silent?.status, .failed)
        XCTAssertEqual(silent?.detail.contains("BVDRIVER"), true)

        let refused = outcome(HvfAgentServiceGate.serviceNotStartedMessage, code: -1)
        XCTAssertEqual(refused?.status, .failed)
        XCTAssertEqual(refused?.detail.contains("exit -1"), true)
        XCTAssertEqual(refused?.detail.contains(HvfAgentServiceGate.serviceNotStartedMessage), true)

        XCTAssertEqual(outcome("BVDRIVER result=maybe detail=x")?.status, .failed)
        XCTAssertEqual(outcome("BVDRIVER pending")?.status, .failed)
        XCTAssertEqual(outcome("BVDRIVER result=failed detail=curl.exe missing"), .init(status: .failed, detail: "curl.exe missing"))
    }
}

@MainActor
final class HvfDisplayDriverInstallSessionTests: XCTestCase {
    private typealias Install = HvfDisplayDriverInstall

    func testActionIsDisabledWhileTheAgentIsNotReady() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let session = fixture.session
        XCTAssertFalse(Install.agentReady(session))
        session.connectionState = .booting
        session.serviceStarted = true
        XCTAssertFalse(Install.agentReady(session))
        session.connectionState = .connected(host: "guest")
        session.serviceStarted = false
        XCTAssertFalse(Install.agentReady(session))
        session.serviceStarted = true
        XCTAssertTrue(Install.agentReady(session))
        session.connectionState = .stopping
        XCTAssertFalse(Install.agentReady(session))
    }

    func testRunnerRefusesWithoutWritingWhenTheAgentIsNotReady() async throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let reply = await Install.runInGuest(Install.launchCommand, session: fixture.session, timeout: 1)
        XCTAssertEqual(reply.code, -1)
        XCTAssertEqual(reply.output, HvfAgentServiceGate.serviceNotStartedMessage)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.control.path))
    }

    /// The launch returns at once; each poll is its own short agent command, so other agent
    /// work runs between them.
    func testInstallLaunchesThenPollsWithShortCommandsUntilTheResult() async {
        var sent: [String] = [], replies = ["BVDRIVER started pid=7", "BVDRIVER running pid=7",
                                           "BVDRIVER result=installed detail=Red Hat VirtIO GPU DOD controller status=OK"]
        let outcome = await Install.install(runner: { command, timeout in
            sent.append(command)
            XCTAssertTrue(timeout <= 30, "\(timeout)")
            return (replies.removeFirst(), 0)
        }, pollInterval: 0.01, timeout: 30)
        XCTAssertEqual(sent, [Install.launchCommand, Install.pollCommand, Install.pollCommand])
        XCTAssertEqual(outcome, .init(status: .installed, detail: "Red Hat VirtIO GPU DOD controller status=OK"))
    }

    func testInstallStopsWaitingAtItsBound() async {
        var polls = 0
        let outcome = await Install.install(runner: { command, _ in
            if command == Install.pollCommand { polls += 1 }
            return ("BVDRIVER running pid=7", 0)
        }, pollInterval: 0.02, timeout: 0.3)
        XCTAssertEqual(outcome, Install.stillRunning)
        XCTAssertFalse(outcome.leavesDriverInGuest)
        XCTAssertGreaterThan(polls, 1)
    }

    func testInstallGoesThroughTheSessionControlFileAndFramedReplies() async throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        fixture.session.serviceStarted = true
        fixture.session.connectionState = .connected(host: "guest")
        try "BVAGENT SERVICE start t=1\n".write(to: fixture.log, atomically: true, encoding: .utf8)
        var outcome: Install.Outcome?
        Task { @MainActor in
            outcome = await Install.install(runner: { await Install.runInGuest($0, session: fixture.session, timeout: $1) },
                                            pollInterval: 0.01, timeout: 20)
        }
        try await fixture.reply(to: Install.launchCommand, with: "BVDRIVER started pid=7", line: 1)
        try await fixture.reply(to: Install.pollCommand, with: "BVDRIVER result=staged detail=no 1AF4:1050 device yet", line: 2)
        let deadline = Date().addingTimeInterval(10)
        while outcome == nil, Date() < deadline { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertEqual(outcome, .init(status: .staged, detail: "no 1AF4:1050 device yet"))
    }

    func testFailedOutcomeDoesNotReachOnInstalled() async {
        var installed = 0
        var sent: [String] = []
        let outcome = await withCheckedContinuation { (done: CheckedContinuation<Install.Outcome, Never>) in
            Install.start(
                runner: { command, _ in sent.append(command); return ("BVDRIVER result=failed detail=pnputil exit 5", 0) },
                onInstalled: { _ in installed += 1 }
            ) { done.resume(returning: $0) }
        }
        XCTAssertEqual(sent, [Install.launchCommand])
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

        /// Waits for the control file's `line`th line to be `command`, then appends the agent's framed reply.
        func reply(to command: String, with body: String, line: Int) async throws {
            let deadline = Date().addingTimeInterval(10)
            while Date() < deadline {
                let lines = ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n")
                if lines.count >= line {
                    XCTAssertEqual(String(lines[line - 1]), command)
                    let handle = try FileHandle(forWritingTo: log)
                    try handle.seekToEnd()
                    try handle.write(contentsOf: Data("BVAGENT CMD \(command) exit=0\n\(body)\nBVAGENT END \(command)\n".utf8))
                    try handle.close()
                    return
                }
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            XCTFail("no control line \(line)")
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}

@MainActor
final class HvfDisplayDriverFlagTests: XCTestCase {
    /// Installed and staged save the per-VM flag through the normal vm.json path when the session
    /// marks the VM stopped (the install itself runs in a running guest), with no view involved:
    /// the user may have selected another VM before Windows shut down. Failed never saves it.
    func testInstalledOrStagedOutcomeIsSavedWhenTheSessionStopsAndFailedIsNot() async throws {
        for (status, expected) in [("installed", true), ("staged", true), ("failed", false)] {
            let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
            defer { fixture.clean() }
            let saved = fixture.config("windows-\(status)"); fixture.save(saved)
            let library = fixture.library(probe: probe)
            let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
            session.connectionState = .connected(host: "guest")
            let outcome = await withCheckedContinuation { (done: CheckedContinuation<HvfDisplayDriverInstall.Outcome, Never>) in
                HvfDisplayDriverInstall.start(runner: { _, _ in ("BVDRIVER result=\(status) detail=x", 0) },
                                              onInstalled: { _ in HvfDisplayDriverFlag.record(session) }) { done.resume(returning: $0) }
            }
            XCTAssertEqual(outcome.status.rawValue, status)
            XCTAssertNil(library.latestConfiguration(for: saved).virtioDisplayDriverInstalled, "\(status): not while running")

            session.markStopped()
            let latest = library.latestConfiguration(for: saved)
            XCTAssertEqual(latest.virtioDisplayDriverInstalled == true, expected, status)
            XCTAssertEqual(try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL)).virtioGpu2d, expected, status)
            // Rebound in place: this session's next start attaches the device, with no re-registration warning.
            XCTAssertEqual(session.config.virtioGpu2d, expected, status)
            XCTAssertTrue(library.hvfRuntimeDetailSession(for: latest) === session, status)
            XCTAssertNil(library.operationError, status)
            XCTAssertNil(session.displayDriverFlagMessage, status)
            XCTAssertFalse(HvfDisplayDriverFlag.pendingVMs.contains(saved.slug), status)
        }
    }

    func testARefusedSaveStaysPendingForTheNextStop() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config("windows-refused"); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        session.connectionState = .connected(host: "guest")
        XCTAssertNil(HvfDisplayDriverFlag.record(session))

        probe.running = true // another process still holds the VM's disk
        session.markStopped()
        XCTAssertNil(library.latestConfiguration(for: saved).virtioDisplayDriverInstalled)
        XCTAssertTrue(HvfDisplayDriverFlag.pendingVMs.contains(saved.slug))
        XCTAssertNotNil(session.displayDriverFlagMessage)

        probe.running = false
        session.markStopped()
        XCTAssertEqual(library.latestConfiguration(for: saved).virtioDisplayDriverInstalled, true)
        XCTAssertNil(session.displayDriverFlagMessage)
        XCTAssertFalse(HvfDisplayDriverFlag.pendingVMs.contains(saved.slug))
    }
}

import Foundation
import XCTest
@testable import BridgeVMControl

/// Growing a stopped VM's disk and extending C: at the next agent start (HvfDiskGrow.swift).
@MainActor
final class HvfDiskGrowTests: XCTestCase {
    private let gib = HvfDiskGrow.bytesPerGiB

    /// Whole GiB at least 1 GiB above the current size (the extend acts only on more than 1000 MiB),
    /// up to the volume's free space minus the 10 GiB margin.
    func testSizesAreWholeGiBAboveTheCurrentSizeThatLeaveTheMarginFree() {
        XCTAssertEqual(HvfDiskGrow.sizeRange(currentBytes: 64 * gib, availableBytes: 100 * gib), 65...154)
        XCTAssertEqual(HvfDiskGrow.sizeRange(currentBytes: 64 * gib + 512, availableBytes: 13 * gib), 66...67)
        XCTAssertNil(HvfDiskGrow.sizeRange(currentBytes: 64 * gib + 512, availableBytes: 11 * gib),
                     "only 65 GiB fits, and it gains less than 1 GiB")
        XCTAssertNil(HvfDiskGrow.sizeRange(currentBytes: 64 * gib, availableBytes: 11 * gib - 1))
        XCTAssertNil(HvfDiskGrow.sizeRange(currentBytes: 64 * gib, availableBytes: 0))
        XCTAssertEqual(HvfDiskGrow.sizeRange(currentBytes: 64 * gib, availableBytes: .max)?.upperBound,
                       Int(UInt64(Int64.max) / gib), "ftruncate's off_t bounds the size")
        XCTAssertEqual(HvfDiskGrow.gibText(72 * gib), "72 GiB")
        XCTAssertEqual(HvfDiskGrow.gibText(gib * 3 / 2), "1.5 GiB")
    }

    /// One agent line under cmd.exe's limit that extends C: to the largest size Windows supports.
    func testTheExtendCommandIsOneAgentLineThatExtendsCToItsLargestSize() throws {
        let command = HvfDiskGrow.extendCommand
        XCTAssertEqual(try HvfGuestCommand.normalizeShell(command).get(), command)
        XCTAssertTrue(command.utf8.count <= HvfClipboardPaste.maximumCommandCharacters, "\(command.utf8.count)")
        let encoded = try XCTUnwrap(command.components(separatedBy: "-EncodedCommand ").last)
        let bytes = try XCTUnwrap(Data(base64Encoded: encoded))
        let script = try XCTUnwrap(String(data: bytes, encoding: .utf16LittleEndian))
        XCTAssertTrue(script.contains("$max = (Get-PartitionSupportedSize -DriveLetter C).SizeMax"), script)
        XCTAssertTrue(script.contains("Resize-Partition -DriveLetter C -Size $max"), script)
        XCTAssertTrue(script.contains("if ($max -le $size + 1000MB) { 'BVDISK result=already"), "idempotent: \(script)")
        for result in ["extended", "already", "failed"] {
            XCTAssertTrue(script.contains("'BVDISK result=\(result) "), result)
        }
    }

    func testTheGuestReportBecomesAShortNotice() {
        XCTAssertEqual(HvfDiskGrow.outcome(reply: "noise\r\nBVDISK result=extended size=77309411328\r\n"),
                       .extended(bytes: 77_309_411_328))
        XCTAssertEqual(HvfDiskGrow.outcome(reply: "BVDISK result=already size=68412243968"),
                       .already(bytes: 68_412_243_968))
        XCTAssertEqual(HvfDiskGrow.outcome(reply: "BVDISK result=failed detail=Size Not Supported"),
                       .failed(detail: "Size Not Supported"))
        XCTAssertNil(HvfDiskGrow.outcome(reply: "HVF 게스트 명령 응답 시간 초과"))
        XCTAssertEqual(HvfDiskGrow.Outcome.extended(bytes: 72 * gib).notice,
                       "C: 드라이브를 늘린 디스크에 맞게 확장했습니다 (72 GiB).")
        XCTAssertEqual(HvfDiskGrow.Outcome.failed(detail: "Size Not Supported").notice,
                       "C: 드라이브를 자동으로 확장하지 못했습니다(Size Not Supported). Windows의 디스크 관리에서 C: 볼륨을 확장하세요.")
        XCTAssertTrue(HvfDiskGrow.pendingNote.contains("디스크 관리"), "the fallback when no agent reports")
    }

    /// Every agent start of a VM the app grew runs the (idempotent) extend; the flag stays. A reply
    /// without a report changes nothing, a report shows its notice.
    func testEveryAgentStartOfAGrownVMChecksAndTheFlagStays() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let session = try XCTUnwrap(fixture.library(probe: probe).hvfRuntimeDetailSession(for: saved))
        let flag = try XCTUnwrap(HvfDiskGrow.grownFlag(config: session.config))
        XCTAssertEqual(flag.path, saved.bundlePath + "/metadata/disk-grown")
        var sent: [String] = []
        var reply = "HVF 게스트 명령 응답 시간 초과"
        let runner: HvfDisplayDriverInstall.Runner = { command, _ in
            sent.append(command)
            return (reply, -1)
        }

        XCTAssertNil(session.extendGrownDisk(runner: runner), "never grown, no command")
        try Data("0\n".utf8).write(to: flag)
        await session.extendGrownDisk(runner: runner)?.value
        XCTAssertEqual(sent, [HvfDiskGrow.extendCommand])
        XCTAssertNil(session.diskGrowNotice, "no report yet")

        reply = "BVDISK result=extended size=\(72 * gib)\n"
        await session.extendGrownDisk(runner: runner)?.value
        XCTAssertEqual(session.diskGrowNotice, HvfDiskGrow.Outcome.extended(bytes: 72 * gib).notice)
        reply = "BVDISK result=already size=\(72 * gib)\n"
        await session.extendGrownDisk(runner: runner)?.value
        XCTAssertEqual(session.diskGrowNotice, HvfDiskGrow.Outcome.already(bytes: 72 * gib).notice)
        XCTAssertEqual(sent.count, 3, "one check per agent start")
        XCTAssertEqual(try String(contentsOf: flag, encoding: .utf8), "0\n", "the flag stays")
    }

    /// Grow, boot (C: extends), then restore a snapshot taken before that boot: the restore brings
    /// back disk and vars only, so the grown disk has an unextended C: again, and the next agent
    /// start extends it. (Before, the extend removed the marker and nothing ran again.)
    func testAnUnextendedCFromARestoredSnapshotIsExtendedAtTheNextStart() async throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let session = try grownBundleSession(fixture)
        let plan = try XCTUnwrap(HvfDiskGrow.plan(config: session.config, repoRoot: session.repoRoot))
        await session.growDisk(plan, toGiB: 72)?.value
        var sent = 0
        let extended: HvfDisplayDriverInstall.Runner = { _, _ in
            sent += 1
            return ("BVDISK result=extended size=\(72 * self.gib)\n", 0)
        }
        await session.extendGrownDisk(runner: extended)?.value  // the boot after the grow
        XCTAssertEqual(sent, 1)
        // The restored snapshot's C: is unextended; the agent's report says so by extending again.
        await session.extendGrownDisk(runner: extended)?.value
        XCTAssertEqual(sent, 2, "the restored, unextended C: is extended at the next start")
        XCTAssertEqual(session.diskGrowNotice, HvfDiskGrow.Outcome.extended(bytes: 72 * gib).notice)
    }

    /// Failed reports are retried at the next starts; the third in a row stops the check with the
    /// Disk Management notice until the next grow.
    func testAFailedExtendIsRetriedAtTheNextStartsUpToThreeTimes() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let session = try XCTUnwrap(fixture.library(probe: probe).hvfRuntimeDetailSession(for: saved))
        let flag = try XCTUnwrap(HvfDiskGrow.grownFlag(config: session.config))
        try Data("0\n".utf8).write(to: flag)
        var sent = 0
        let failed: HvfDisplayDriverInstall.Runner = { _, _ in
            sent += 1
            return ("BVDISK result=failed detail=The disk is not ready\n", 0)
        }
        for _ in 1...2 {
            await session.extendGrownDisk(runner: failed)?.value
            XCTAssertEqual(session.diskGrowNotice,
                           "C: 드라이브를 자동으로 확장하지 못했습니다(The disk is not ready). 다음 Windows 시작 때 다시 시도합니다.")
        }
        await session.extendGrownDisk(runner: failed)?.value
        XCTAssertEqual(session.diskGrowNotice,
                       "C: 드라이브를 자동으로 확장하지 못했습니다(The disk is not ready). Windows의 디스크 관리에서 C: 볼륨을 확장하세요.")
        XCTAssertNil(session.extendGrownDisk(runner: failed), "no fourth attempt")
        XCTAssertEqual(sent, 3)
        XCTAssertTrue(FileManager.default.fileExists(atPath: flag.path), "the flag is never removed")
        try Data("0\n".utf8).write(to: flag)  // what the next grow writes
        await session.extendGrownDisk(runner: failed)?.value
        XCTAssertEqual(sent, 4, "a new grow checks again")
    }

    /// A session booting a managed bundle under `fixture.root`, whose bundled helper reports any grow.
    private func grownBundleSession(_ fixture: HvfOwnedRuntimeFixture) throws -> HvfEngineSession {
        let bundle = fixture.root.appendingPathComponent("vm/bundle")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw"), vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let helper = fixture.root.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        for url in [disk, vars, helper] {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: Data(count: 512)))
        }
        try Data("#!/bin/sh\necho \"disk_bytes $4\"\n".utf8).write(to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helper.path)
        fixture.config.targetDiskPath = disk.path
        fixture.config.uefiVarsPath = vars.path
        return fixture.session()
    }

    /// The grow's message stays until the guest reports; each report then replaces it, a retried
    /// failure first and the extended result after it.
    func testTheGuestReportReplacesTheGrowMessage() async throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let session = try grownBundleSession(fixture)
        let plan = try XCTUnwrap(HvfDiskGrow.plan(config: session.config, repoRoot: session.repoRoot))

        await session.growDisk(plan, toGiB: 72)?.value
        let grew = "디스크를 72 GiB로 늘렸습니다. " + HvfDiskGrow.pendingNote
        XCTAssertEqual(session.diskGrowNotice, grew)
        XCTAssertNil(session.mutationReservation, "the reservation ends with the grow")

        var reply = "HVF 게스트 명령 응답 시간 초과"
        let runner: HvfDisplayDriverInstall.Runner = { _, _ in (reply, 0) }
        await session.extendGrownDisk(runner: runner)?.value
        XCTAssertEqual(session.diskGrowNotice, grew, "no report yet")
        reply = "BVDISK result=failed detail=busy"
        await session.extendGrownDisk(runner: runner)?.value
        XCTAssertEqual(session.diskGrowNotice, HvfDiskGrow.Outcome.failed(detail: "busy").retryNotice)
        reply = "BVDISK result=extended size=\(72 * gib)"
        await session.extendGrownDisk(runner: runner)?.value
        XCTAssertEqual(session.diskGrowNotice, HvfDiskGrow.Outcome.extended(bytes: 72 * gib).notice)
    }

    /// `BVAGENT SERVICE start` sends the extend through agent.ctl only for a VM the app grew.
    func testServiceStartSendsTheExtendOnlyAfterAGrow() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let session = try XCTUnwrap(fixture.library(probe: probe).hvfRuntimeDetailSession(for: saved))
        let control = URL(fileURLWithPath: session.config.ctlFilePath)
        func extendsSent() -> Int {
            ((try? String(contentsOf: control, encoding: .utf8)) ?? "")
                .split(separator: "\n").filter { $0 == HvfDiskGrow.extendCommand }.count
        }
        session.connectionState = .connected(host: "guest")
        session.handle(.serviceStart(tMs: 1))
        XCTAssertFalse(session.diskExtendInFlight)

        try Data("1\n".utf8).write(to: XCTUnwrap(HvfDiskGrow.grownFlag(config: session.config)))
        session.handle(.serviceStart(tMs: 2))
        session.handle(.serviceStart(tMs: 3))  // an agent restart while the first extend waits
        for _ in 0..<40 where extendsSent() == 0 { try await Task.sleep(nanoseconds: 50_000_000) }
        XCTAssertEqual(extendsSent(), 1)

        session.connectionState = .stopped  // the reply never comes: the wait ends with the guest
        for _ in 0..<40 where session.diskExtendInFlight { try await Task.sleep(nanoseconds: 50_000_000) }
        XCTAssertFalse(session.diskExtendInFlight)
        XCTAssertNil(session.diskGrowNotice)
    }

    /// The bundled helper grows the bundle's managed media and the flag follows; a VM holding the
    /// media (exit status 3) gets the stop-first message and no flag.
    func testAGrowRunsTheHelperOnTheBundleMediaAndSetsTheFlag() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("disk-grow-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let repo = root.appendingPathComponent("repo"), bundle = root.appendingPathComponent("vm/bundle")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        for url in [disk, vars, helper] {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        }
        try Data(count: 512).write(to: disk)
        try Data(count: 16).write(to: vars)
        let script = """
            #!/bin/sh
            echo "$@" >> "$0.args"
            [ -e "$0.in-use" ] && { echo 'grow: media is in use by a running VM' >&2; exit 3; }
            [ "$1" = size ] && { printf 'disk_bytes 68719476736\\nvars_bytes 16\\n'; exit 0; }
            echo "disk_bytes $4"
            """
        XCTAssertTrue(FileManager.default.createFile(atPath: helper.path, contents: Data(script.utf8)))
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helper.path)
        var config = HvfEngineConfig(
            targetDiskPath: disk.path, uefiVarsPath: vars.path,
            evidenceDir: bundle.appendingPathComponent("logs/hvf").path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4,
            clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: bundle.appendingPathComponent("metadata/hvf.ctl").path,
            vtpmStateDir: nil, vtpmKeyID: "test-vm", allowsExperimental3D: false)
        let plan = try XCTUnwrap(HvfDiskGrow.plan(config: config, repoRoot: repo))
        let marker = bundle.appendingPathComponent("metadata/disk-grown")
        XCTAssertEqual(plan.grownFlag, marker)

        XCTAssertEqual(try HvfDiskGrow.currentBytes(plan), 64 * gib)
        XCTAssertEqual(try HvfDiskGrow.grow(plan, to: 72 * gib), 72 * gib)
        XCTAssertEqual(try String(contentsOf: marker, encoding: .utf8), "0\n", "no failed report yet")
        XCTAssertEqual(try String(contentsOf: URL(fileURLWithPath: helper.path + ".args"), encoding: .utf8),
                       "size \(disk.path) \(vars.path)\ngrow \(disk.path) \(vars.path) \(72 * gib)\n")

        try FileManager.default.removeItem(at: marker)
        XCTAssertTrue(FileManager.default.createFile(atPath: helper.path + ".in-use", contents: nil))
        XCTAssertThrowsError(try HvfDiskGrow.grow(plan, to: 80 * gib)) { error in
            XCTAssertEqual(error.localizedDescription, HvfDiskGrow.stopFirst)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))

        config.targetDiskPath = root.appendingPathComponent("elsewhere.raw").path
        XCTAssertNil(HvfDiskGrow.plan(config: config, repoRoot: repo), "only the bundle's managed media")
        XCTAssertNil(HvfDiskGrow.grownFlag(config: config))
    }
}

import Foundation
import XCTest
@testable import BridgeVMControl

/// Growing a stopped VM's disk and extending C: at the next agent start (HvfDiskGrow.swift).
@MainActor
final class HvfDiskGrowTests: XCTestCase {
    private let gib = HvfDiskGrow.bytesPerGiB

    /// Whole GiB above the current size, up to the volume's free space minus the 10 GiB margin.
    func testSizesAreWholeGiBAboveTheCurrentSizeThatLeaveTheMarginFree() {
        XCTAssertEqual(HvfDiskGrow.sizeRange(currentBytes: 64 * gib, availableBytes: 100 * gib), 65...154)
        XCTAssertEqual(HvfDiskGrow.sizeRange(currentBytes: 64 * gib + 512, availableBytes: 12 * gib), 65...66)
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

    /// After a grow, the agent start runs the extend; a reply without a report keeps the marker for
    /// the next start, a report removes it and shows the notice, and nothing runs again.
    func testAnAgentStartAfterAGrowExtendsCOnceAndClearsTheMarker() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let session = try XCTUnwrap(fixture.library(probe: probe).hvfRuntimeDetailSession(for: saved))
        let marker = try XCTUnwrap(HvfDiskGrow.pendingMarker(config: session.config))
        XCTAssertEqual(marker.path, saved.bundlePath + "/metadata/disk-grow-pending")
        var sent: [String] = []
        var reply = "HVF 게스트 명령 응답 시간 초과"
        let runner: HvfDisplayDriverInstall.Runner = { command, _ in
            sent.append(command)
            return (reply, -1)
        }

        XCTAssertNil(session.extendGrownDiskIfPending(runner: runner), "no grow, no command")
        try Data("77309411328\n".utf8).write(to: marker)
        await session.extendGrownDiskIfPending(runner: runner)?.value
        XCTAssertEqual(sent, [HvfDiskGrow.extendCommand])
        XCTAssertTrue(FileManager.default.fileExists(atPath: marker.path), "no report: the next agent start tries again")
        XCTAssertNil(session.diskGrowNotice)

        reply = "BVDISK result=extended size=\(72 * gib)\n"
        await session.extendGrownDiskIfPending(runner: runner)?.value
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))
        XCTAssertEqual(session.diskGrowNotice, HvfDiskGrow.Outcome.extended(bytes: 72 * gib).notice)
        XCTAssertNil(session.extendGrownDiskIfPending(runner: runner), "once")
        XCTAssertEqual(sent.count, 2)
    }

    /// `BVAGENT SERVICE start` sends the extend through agent.ctl only while a grow is pending.
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

        try Data("1\n".utf8).write(to: XCTUnwrap(HvfDiskGrow.pendingMarker(config: session.config)))
        session.handle(.serviceStart(tMs: 2))
        session.handle(.serviceStart(tMs: 3))  // an agent restart while the first extend waits
        for _ in 0..<40 where extendsSent() == 0 { try await Task.sleep(nanoseconds: 50_000_000) }
        XCTAssertEqual(extendsSent(), 1)

        session.connectionState = .stopped  // the reply never comes: the wait ends with the guest
        for _ in 0..<40 where session.diskExtendInFlight { try await Task.sleep(nanoseconds: 50_000_000) }
        XCTAssertFalse(session.diskExtendInFlight)
        XCTAssertNil(session.diskGrowNotice)
    }

    /// The bundled helper grows the bundle's managed media and the marker follows; a VM holding the
    /// media (exit status 3) gets the stop-first message and no marker.
    func testAGrowRunsTheHelperOnTheBundleMediaAndLeavesTheMarker() throws {
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
        let marker = bundle.appendingPathComponent("metadata/disk-grow-pending")
        XCTAssertEqual(plan.pendingMarker, marker)

        XCTAssertEqual(try HvfDiskGrow.currentBytes(plan), 64 * gib)
        XCTAssertEqual(try HvfDiskGrow.grow(plan, to: 72 * gib), 72 * gib)
        XCTAssertEqual(try String(contentsOf: marker, encoding: .utf8), "\(72 * gib)\n")
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
        XCTAssertNil(HvfDiskGrow.pendingMarker(config: config))
    }
}

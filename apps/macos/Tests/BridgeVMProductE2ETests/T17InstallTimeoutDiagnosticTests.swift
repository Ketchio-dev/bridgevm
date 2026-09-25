import Darwin
import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class T17InstallTimeoutDiagnosticTests: XCTestCase {
    private final class Clock {
        var now = 0.0
        let step: Double
        init(step: Double = 60) { self.step = step }
    }

    private func wait(
        clock: Clock, timeout: Double = 120,
        running: () -> Bool = { true },
        runtime: (String) throws -> Void = { _ in
            throw T17Blocker(code: "ui-element-missing",
                             detail: "required accessibility identifier was not found: expected")
        },
        stage: () throws -> String,
        failure: () throws -> String = { "" },
        sample: () -> T17InstallTimeoutSample = { .unavailable },
        markers: () -> T17InstallMarkerCounts? = { nil }
    ) throws {
        try T17InstallTimeoutDiagnostic.wait(
            timeout: timeout, clock: { clock.now }, pause: { _ in clock.now += clock.step },
            applicationIsRunning: running, runtimeView: runtime, stage: stage,
            failure: failure, sample: sample, markers: markers)
    }

    func testTimeoutRetainsPhaseQueryErrorsAndCorrelatedHostLogSamples() {
        let clock = Clock()
        var samples = 0
        XCTAssertThrowsError(try wait(clock: clock, stage: {
            if clock.now == 0 { return "Windows 무인 설치" }
            throw T17Blocker(code: "ui-element-missing",
                             detail: "ax_tree_read_failed;ax_error=-25202;secret=/private/guest.iso")
        }, sample: {
            defer { samples += 1 }
            return .init(hostTotalTicks: UInt64(samples * 100),
                         hostIdleTicks: UInt64(samples == 2 ? 20 : 0),
                         loadPerCoreX100: samples == 1 ? 450 : 100,
                         logStatus: .present, logBytes: samples == 0 ? 100 : 120)
        }, markers: { .init(boot: 1, dism: 2, shutdown: 0, watchdog: 0) })) { error in
            let blocker = error as? T17Blocker
            XCTAssertEqual(blocker?.code, "installer-failed")
            let detail = blocker?.detail ?? ""
            for token in ["diag=v1", "phase=installing", "phase_age_s=120",
                          "ax_stage_ok=1", "ax_stage_err=1", "ax_runtime_err=0",
                          "log_bytes=120", "log_growth_age_s=60", "sample_age_s=0",
                          "boot=1,dism=2,shutdown=0,watchdog=0",
                          "host_idle_min_pct=0", "load_per_core_max_x100=450", "samples=3"] {
                XCTAssertTrue(detail.contains(token), token)
            }
            XCTAssertFalse(detail.contains("/private/guest.iso"))
            XCTAssertLessThanOrEqual(detail.utf8.count, 512)
        }
        XCTAssertEqual(samples, 3)
    }

    func testExpectedMissingIdentifierIsNotCountedAsAXReadError() {
        let clock = Clock()
        XCTAssertThrowsError(try wait(clock: clock, stage: {
            if clock.now == 0 { return "설치 소스 준비" }
            throw T17Blocker(code: "ui-element-missing",
                             detail: "required accessibility identifier was not found: stage")
        })) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("phase=preparing-source"))
            XCTAssertTrue(detail.contains("ax_stage_missing=1"))
            XCTAssertTrue(detail.contains("ax_stage_err=0"))
        }
    }

    func testCompletionFailureAndAppExitKeepExistingDecisions() {
        let clock = Clock()
        var markerCalls = 0
        XCTAssertNoThrow(try wait(clock: clock, stage: { "완료" }, markers: {
            markerCalls += 1; return nil
        }))
        XCTAssertEqual(markerCalls, 0)
        XCTAssertThrowsError(try wait(clock: clock, stage: { "실패" },
                                      failure: { " measured failure " })) { error in
            XCTAssertEqual(error as? T17Blocker,
                           T17Blocker(code: "installer-failed", detail: "measured failure"))
        }
        XCTAssertThrowsError(try wait(clock: clock, stage: { "실패" },
                                      failure: { T17InstallTimeoutDiagnostic.timeoutDetail },
                                      markers: { XCTFail("not a timeout"); return nil })) { error in
            XCTAssertEqual(error as? T17Blocker,
                           T17Blocker(code: "installer-failed",
                                      detail: T17InstallTimeoutDiagnostic.timeoutDetail))
        }
        XCTAssertThrowsError(try wait(clock: clock, running: { false }, stage: { "설치 소스 준비" })) { error in
            XCTAssertEqual(error as? T17Blocker,
                           T17Blocker(code: "installer-failed", detail: "product app exited during installation"))
        }
    }

    func testRuntimeSuccessKeepsIdentifierOrderAndSkipsDiagnostics() {
        let clock = Clock()
        var calls: [String] = []
        XCTAssertNoThrow(try wait(clock: clock, runtime: { identifier in
            calls.append(identifier)
            if identifier == "bridgevm.windows.runtime.view" {
                throw T17Blocker(code: "ui-element-missing",
                                 detail: "required accessibility identifier was not found: runtime")
            }
        }, stage: { calls.append("stage"); return "Windows 무인 설치" },
            markers: { XCTFail("markers must be timeout-only"); return nil }))
        XCTAssertEqual(calls, ["stage", "bridgevm.windows.runtime.view", "bridgevm.dashboard.advanced"])
    }

    func testSamplingStopsAtThirtyTwoAndDetailContainsNoArbitraryText() {
        let clock = Clock()
        var samples = 0
        XCTAssertThrowsError(try wait(clock: clock, timeout: 6_000, stage: {
            throw T17Blocker(code: "ui-element-missing",
                             detail: "ax_tree_read_failed;guest=secret;path=/private/media.iso")
        }, sample: {
            samples += 1
            return .init(hostTotalTicks: nil, hostIdleTicks: nil,
                         loadPerCoreX100: 10_000, logStatus: .unavailable, logBytes: nil)
        })) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("samples=32"))
            XCTAssertTrue(detail.contains("phase=unknown"))
            XCTAssertFalse(detail.contains("secret"))
            XCTAssertFalse(detail.contains("/private/"))
            XCTAssertLessThanOrEqual(detail.utf8.count, 512)
        }
        XCTAssertEqual(samples, 32)
    }

    func testTimeoutWithinOneMinuteUsesPreviousSample() {
        let clock = Clock(step: 45)
        var samples = 0
        XCTAssertThrowsError(try wait(clock: clock, timeout: 100, stage: {
            "Windows 무인 설치"
        }, sample: {
            samples += 1
            return .unavailable
        })) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("samples=2"))
            XCTAssertTrue(detail.contains("sample_age_s=45"))
        }
        XCTAssertEqual(samples, 2)
    }

    func testExtremeInjectedCountersStayBoundedWithoutReplacingTimeout() {
        let clock = Clock()
        var samples = 0
        XCTAssertThrowsError(try wait(clock: clock, timeout: 60, stage: {
            "Windows 무인 설치"
        }, sample: {
            defer { samples += 1 }
            return .init(hostTotalTicks: samples == 0 ? 0 : UInt64.max,
                         hostIdleTicks: samples == 0 ? 0 : UInt64.max,
                         loadPerCoreX100: Int.max, logStatus: .present,
                         logBytes: UInt64.max)
        }, markers: {
            .init(boot: Int.max, dism: -1, shutdown: Int.max, watchdog: Int.max)
        })) { error in
            let blocker = error as? T17Blocker
            XCTAssertEqual(blocker?.code, "installer-failed")
            let detail = blocker?.detail ?? ""
            XCTAssertTrue(detail.hasPrefix(T17InstallTimeoutDiagnostic.timeoutDetail))
            XCTAssertTrue(detail.contains("boot=9999,dism=0,shutdown=9999,watchdog=9999"))
            XCTAssertTrue(detail.contains("host_idle_min_pct=100"))
            XCTAssertLessThanOrEqual(detail.utf8.count, 512)
        }
    }

    func testLogSamplerRejectsSymlinkAndCountsOnlyBoundedTailMarkers() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let outside = root.deletingLastPathComponent().appendingPathComponent(UUID().uuidString)
        try Data("private guest text".utf8).write(to: outside)
        defer { try? FileManager.default.removeItem(at: outside) }
        let runLog = root.appendingPathComponent("run.log")
        try FileManager.default.createSymbolicLink(at: runLog, withDestinationURL: outside)
        let sampler = T17InstallEnvironmentSampler(evidenceDirectory: root)
        XCTAssertEqual(sampler.capture().logStatus, .unavailable)
        XCTAssertNil(sampler.markers())
        let clock = Clock()
        XCTAssertThrowsError(try wait(clock: clock, timeout: 0, stage: { "Windows 무인 설치" },
                                      sample: { sampler.capture() }, markers: { sampler.markers() })) { error in
            XCTAssertEqual((error as? T17Blocker)?.code, "installer-failed")
            XCTAssertTrue((error as? T17Blocker)?.detail.contains("run_log=unavailable") == true)
        }
        try FileManager.default.removeItem(at: runLog)
        XCTAssertEqual(sampler.capture().logStatus, .unavailable)
        try FileManager.default.createSymbolicLink(at: runLog, withDestinationURL: outside)
        let directoryLink = root.deletingLastPathComponent().appendingPathComponent(UUID().uuidString)
        try FileManager.default.createSymbolicLink(at: directoryLink, withDestinationURL: root)
        defer { try? FileManager.default.removeItem(at: directoryLink) }
        XCTAssertEqual(T17InstallEnvironmentSampler(evidenceDirectory: directoryLink).capture().logStatus,
                       .unavailable)
        try FileManager.default.removeItem(at: runLog)
        XCTAssertEqual(Darwin.mkfifo(runLog.path, 0o600), 0)
        XCTAssertEqual(sampler.capture().logStatus, .unavailable)
        try FileManager.default.removeItem(at: runLog)
        var bytes = Data("BOOT_TIMER ramfb source=old\n".utf8)
        bytes.append(Data(repeating: 65, count: 65_536))
        bytes.append(Data("untrusted DISM active\nBVINSTALL DISM APPLY fake\nBVINSTALL DISM APPLY\nstop: PSCI 0x84000008 (system off)\nprefix stop: watchdog (CANCELED)\nstop: watchdog (CANCELED) extra\nstop: watchdog (CANCELED)\n".utf8))
        try bytes.write(to: runLog)
        XCTAssertEqual(sampler.capture().logStatus, .present)
        XCTAssertEqual(sampler.capture().logBytes, UInt64(bytes.count))
        let markers = sampler.markers()
        XCTAssertEqual(markers?.boot, 0)
        XCTAssertEqual(markers?.dism, 1)
        XCTAssertEqual(markers?.shutdown, 1)
        XCTAssertEqual(markers?.watchdog, 1)
    }
}

import XCTest
@testable import BridgeVMProductE2E

final class T17MissingIdentifierDiagnosticTests: XCTestCase {
    private typealias Probe = T17MissingIdentifierDiagnostic
    private enum ReadError: Error { case failed }

    func testDashboardFailureKeepsCodePrefixTimeoutAndBoundedMetadata() {
        var sample = Probe.Observation()
        sample.pidProbe = "present"; sample.appPresent = true
        sample.active = false; sample.frontmost = false
        sample.windowsStatus = 0; sample.windowsCount = 0
        sample.focusStatus = -25205; sample.mainStatus = -25205
        sample.inventory = .init(nodes: 5, limited: false, errors: 1,
                                  labels: [.init(role: "AXButton", identifier: "bridgevm.dashboard.advanced")])
        let blocker = Probe.failure(identifier: "bridgevm.dashboard.advanced", timeout: 60, observation: sample)
        XCTAssertEqual(blocker.code, "ui-element-missing")
        XCTAssertTrue(blocker.detail.hasPrefix("required accessibility identifier was not found: bridgevm.dashboard.advanced; windows=0 timeout_s=60.0"))
        XCTAssertTrue(blocker.detail.contains("pid_probe=present,ns_app=true,active=false,front=false"))
        XCTAssertTrue(blocker.detail.contains("ax_windows=0/0"))
        XCTAssertTrue(blocker.detail.contains("bridgevm.dashboard.advanced:AXButton"))
        XCTAssertLessThanOrEqual(blocker.detail.utf8.count, 512)
    }

    func testAXWindowsErrorIsUnansweredRatherThanFalseZero() {
        var sample = Probe.Observation()
        sample.windowsStatus = -25204; sample.windowsCount = nil
        sample.focusStatus = -25205; sample.mainStatus = -25205
        let detail = Probe.failure(identifier: "bridgevm.dashboard.advanced", timeout: 60,
                                   observation: sample).detail
        XCTAssertTrue(detail.contains("windows=unanswered timeout_s=60.0"))
        XCTAssertTrue(detail.contains("ax_windows=-25204/unknown"))
        XCTAssertTrue(detail.contains("ax_focus=-25205/false"))
        XCTAssertTrue(detail.contains("ax_main=-25205/false"))
        sample.windowsStatus = 0; sample.windowsCount = -1
        let malformed = Probe.failure(identifier: "bridgevm.dashboard.advanced", timeout: 60, observation: sample).detail
        XCTAssertTrue(malformed.contains("windows=unanswered timeout_s=60.0"))
    }

    func testInventoryDeduplicatesCycleAndCapsLongTree() {
        let cycle = Probe.inventory(roots: [0], budget: { true },
            metadata: { _ in ("AXButton", "bridgevm.dashboard.advanced") },
            related: { [$0 == 0 ? 1 : 0] }, same: ==)
        XCTAssertEqual(cycle.nodes, 2)
        XCTAssertFalse(cycle.limited)
        XCTAssertEqual(cycle.labels.count, 1)
        let long = Probe.inventory(roots: [0], budget: { true },
            metadata: { _ in ("AXButton", "bridgevm.dashboard.advanced") },
            related: { [$0 + 1] }, same: ==)
        XCTAssertEqual(long.nodes, 128)
        XCTAssertTrue(long.limited)
        XCTAssertEqual(long.labels.count, 1)
    }

    func testDiagnosticReadFailureKeepsOriginalBlockerAndNoPrivateText() {
        let failed = Probe.inventory(roots: [0], budget: { true },
            metadata: { _ in throw ReadError.failed },
            related: { _ in throw ReadError.failed }, same: ==)
        XCTAssertEqual(failed.errors, 2)
        var sample = Probe.Observation()
        sample.pidProbe = "/private/media.iso"
        sample.windowsStatus = -25204
        sample.inventory = failed
        sample.inventory.labels = [
            .init(role: "/private/window-title", identifier: "bridgevm.dashboard.advanced"),
            .init(role: "AXButton", identifier: "/private/guest.iso"),
        ]
        let blocker = Probe.failure(identifier: "/private/request.iso", timeout: 60, observation: sample)
        XCTAssertEqual(blocker.code, "ui-element-missing")
        XCTAssertTrue(blocker.detail.hasPrefix("required accessibility identifier was not found: other; windows=unanswered timeout_s=60.0"))
        XCTAssertTrue(blocker.detail.contains("pid_probe=unknown"))
        XCTAssertTrue(blocker.detail.contains("errors=2"))
        XCTAssertFalse(blocker.detail.contains("/private/"))
        XCTAssertFalse(blocker.detail.contains("window-title"))
        XCTAssertLessThanOrEqual(blocker.detail.utf8.count, 512)
        XCTAssertTrue(blocker.detail.utf8.allSatisfy { $0 < 128 })
    }

    func testBudgetExpirationIsExplicitAndPreservesFailureCode() {
        let stopped = Probe.inventory(roots: [0], budget: { false },
            metadata: { _ in XCTFail("expired budget read metadata"); return (nil, nil) },
            related: { _ in XCTFail("expired budget read children"); return [] }, same: ==)
        XCTAssertEqual(stopped.nodes, 0)
        XCTAssertTrue(stopped.limited)
        var sample = Probe.Observation(); sample.inventory = stopped
        let blocker = Probe.failure(identifier: "bridgevm.dashboard.advanced", timeout: 60, observation: sample)
        XCTAssertEqual(blocker.code, "ui-element-missing")
        XCTAssertTrue(blocker.detail.contains("limited=true"))
    }
}

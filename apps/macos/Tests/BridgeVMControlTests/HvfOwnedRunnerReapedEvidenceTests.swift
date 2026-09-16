import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOwnedRunnerReapedEvidenceTests: XCTestCase {
    private func summary(pid: Int32, role: String = "swtpm", reaped: UInt64 = 1,
                         status: Int32? = 0) -> HvfOwnedRuntimeRoleSummary {
        .init(spawnedCount: 1, reapedCount: reaped,
              last: .init(role: role, pid: pid, generation: nil, reason: "exit", status: status))
    }
    func testAlreadyReapedRealChildDoesNotRequireLateKqueueRegistration() throws {
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/true")
        try child.run(); child.waitUntilExit()
        XCTAssertEqual(child.terminationStatus, 0)
        XCTAssertTrue(HvfOwnedRunnerReapedEvidence.confirmed(summary(pid: child.processIdentifier), role: "swtpm", witness: nil))
    }
    func testLivePIDCannotBeReplacedByAClaimedReap() {
        XCTAssertFalse(HvfOwnedRunnerReapedEvidence.confirmed(summary(pid: getpid()), role: "swtpm", witness: nil))
    }
    func testMissingAndMismatchedReapEvidenceRefuses() throws {
        let child = Process(); child.executableURL = URL(fileURLWithPath: "/usr/bin/true")
        try child.run(); child.waitUntilExit(); let pid = child.processIdentifier
        XCTAssertTrue(HvfOwnedRunnerReapedEvidence.confirmed(summary(pid: pid), role: "swtpm", witness: nil))
        for value in [summary(pid: 0), summary(pid: -1), summary(pid: pid, reaped: 0),
                      summary(pid: pid, status: nil), summary(pid: pid, role: "helper"),
                      .init(spawnedCount: 1, reapedCount: 1, last: nil),
                      .init(spawnedCount: 2, reapedCount: 2, last: summary(pid: pid).last)] {
            XCTAssertFalse(HvfOwnedRunnerReapedEvidence.confirmed(value, role: "swtpm", witness: nil))
        }
    }
    func testZeroChildrenRequiresEmptyBalancedSummary() {
        XCTAssertTrue(HvfOwnedRunnerReapedEvidence.confirmed(.init(spawnedCount: 0, reapedCount: 0, last: nil), role: "helper", witness: nil))
        XCTAssertFalse(HvfOwnedRunnerReapedEvidence.confirmed(.init(spawnedCount: 0, reapedCount: 1, last: nil), role: "helper", witness: nil))
        XCTAssertFalse(HvfOwnedRunnerReapedEvidence.confirmed(.init(spawnedCount: 0, reapedCount: 0, last: summary(pid: getpid()).last), role: "swtpm", witness: nil))
    }
}

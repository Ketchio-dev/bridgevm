import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfSessionInputRouterTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)
    private let binding = ["disk", "vars", "evidence", "control"]

    private func negotiate(_ router: inout HvfSessionInputRouter) {
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ").last ?? "missing"
        let lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 2 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                     "BVAGENT END \(command)"]
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
    }

    func testOwnedUntaintedBootCanActivateButUnknownAttachmentCannot() {
        var router = HvfSessionInputRouter()
        router.attachUnknown(binding: binding)
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) {
            _ in XCTFail("unknown history negotiated"); return true
        }
        XCTAssertEqual(router.route(.text("a"), binding: binding, now: now), .legacy)
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.state, .ready)
        XCTAssertEqual(router.route(.text("a"), binding: binding, now: now), .queued)
        XCTAssertFalse(router.allowLegacyWrite(binding: binding))
    }

    func testAnyLegacyAdmissionPreventsLaterSwitchForThatBoot() {
        for directWrite in [false, true] {
            var router = HvfSessionInputRouter()
            router.beginOwnedBoot(binding: binding)
            if directWrite { XCTAssertTrue(router.allowLegacyWrite(binding: binding)) }
            else { XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .legacy) }
            _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) {
                _ in XCTFail("legacy admission was forgotten"); return true
            }
            XCTAssertEqual(router.route(.pointer("click:0x0"), binding: binding, now: now), .legacy)
        }
    }

    func testTargetChangeRequiresRenegotiationAndNeverFallsBackToHID() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.text("old target"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.cancelTarget(), .cancelled(.targetChanged, discarded: 1))
        XCTAssertEqual(router.route(.text("too early"), binding: binding, now: now), .refused)
        XCTAssertFalse(router.allowLegacyWrite(binding: binding))
        negotiate(&router)
        XCTAssertEqual(router.route(.text("new target"), binding: binding, now: now), .queued)
    }

    func testRestartAndBindingChangeCancelAndRefuseUntilFreshOwnedBoot() {
        for changedBinding in [false, true] {
            var router = HvfSessionInputRouter()
            router.beginOwnedBoot(binding: binding)
            negotiate(&router)
            XCTAssertEqual(router.route(.text("pending"), binding: binding, now: now), .queued)
            _ = router.poll(binding: changedBinding ? ["other"] : binding, serviceReady: true,
                            lines: changedBinding ? [] : ["PSCI_SYSTEM_RESET"], now: now) {
                _ in XCTFail("sent across changed ownership"); return true
            }
            XCTAssertTrue(router.failed)
            XCTAssertEqual(router.count, 0)
            XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .refused)
            router.beginOwnedBoot(binding: binding)
            negotiate(&router)
            XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .queued)
        }
    }

    func testTransportFailureDoesNotFallBackOrReplay() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.pointer("click:0x0"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 1))
        XCTAssertTrue(router.failed)
        XCTAssertEqual(router.route(.text("do not replay"), binding: binding, now: now), .refused)
        XCTAssertFalse(router.allowLegacyWrite(binding: binding))
    }
}

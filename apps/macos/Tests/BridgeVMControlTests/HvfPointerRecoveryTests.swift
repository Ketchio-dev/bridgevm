import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerRecoveryTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)
    private let binding = ["disk", "vars", "evidence", "control"]

    private func ready() -> HvfSessionInputRouter {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ")[1]
        let lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                     "BVAGENT END \(command)"]
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        return router
    }

    private func receipt(_ command: String, count: Int) -> [String] {
        let fields = command.split(separator: " ")
        let label = fields.prefix(2).joined(separator: " ")
        return ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(fields[1]) \(count)", "BVAGENT END \(label)"]
    }

    func testSentPressGetsOneCorrelatedCleanupInsteadOfDroppingQueuedRelease() {
        var router = ready()
        XCTAssertEqual(router.route(.pointer("rightpress:1x2"), binding: binding, now: now), .queued)
        var press = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { press = $0; return true }
        XCTAssertEqual(router.route(.text("discard"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.route(.pointer("releaseall:1x2"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.cancelTarget(now: now), .cancelled(.targetChanged, discarded: 3))
        XCTAssertEqual(router.count, 1)
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .refused)
        var cleanup = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: receipt(press, count: 1), now: now) {
            cleanup = $0; return true
        }
        XCTAssertNotEqual(cleanup, press)
        XCTAssertEqual(Data(base64Encoded: String(cleanup.split(separator: " ")[2])), Data("releaseall:1x2".utf8))
        _ = router.cancelTarget(now: now)
        XCTAssertEqual(router.poll(binding: binding, serviceReady: true, lines: receipt(press, count: 1), now: now) {
            _ in XCTFail("cleanup resent"); return true
        }, .waiting)
        _ = router.poll(binding: binding, serviceReady: true, lines: receipt(cleanup, count: 1), now: now) {
            _ in XCTFail("negotiated before cleanup completed"); return true
        }
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .queued)
        var next = ""; _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { next = $0; return true }
        XCTAssertTrue(next.hasPrefix("KEYINPUT "), "input resumes after cleanup without renegotiation")
    }

    func testCleanupFailureRefusesFurtherInputAndLegacyFallback() {
        var router = ready()
        _ = router.route(.pointer("press:0x0"), binding: binding, now: now)
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in true }
        _ = router.cancelTarget(now: now)
        var cleanup = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { cleanup = $0; return true }
        XCTAssertEqual(router.poll(binding: binding, serviceReady: true, lines: receipt(cleanup, count: 0), now: now) {
            _ in XCTFail("cleanup replay"); return true
        }, .cancelled(.transportFailed, discarded: 1))
        XCTAssertTrue(router.failed)
        XCTAssertFalse(router.allowLegacyWrite("KEY enter", binding: binding, now: now))
        XCTAssertEqual(router.route(.text("blocked"), binding: binding, now: now), .refused)
    }

    /// On a Windows secure desktop the agent inserts nothing, the cleanup's release included: it stays owed for the
    /// next target change instead of failing the route, and input meanwhile takes the HID route.
    func testACleanupReleaseThatInsertsNothingStaysOwedInsteadOfFailingTheRoute() {
        var router = ready()
        XCTAssertEqual(router.route(.pointer("press:1x2"), binding: binding, now: now), .queued)
        var press = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { press = $0; return true }
        XCTAssertNotNil(router.poll(binding: binding, serviceReady: true, lines: receipt(press, count: 1), now: now) { _ in false })
        _ = router.cancelTarget(now: now)
        var cleanup = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { cleanup = $0; return true }
        let id = cleanup.split(separator: " ")[1]
        let nothing = ["BVAGENT CMD POINTERINPUT \(id) exit=1", "BVINPUT_FAILED \(id) Exception calling \"InsertPointer\" with "
                       + "\"1\" argument(s): \"unicode-input-incomplete inserted=0 requested=1\"", "BVAGENT END POINTERINPUT \(id)"]
        XCTAssertEqual(router.poll(binding: binding, serviceReady: true, lines: nothing, now: now) {
            _ in XCTFail("cleanup replay"); return true
        }, .cancelled(.notInserted, discarded: 0))
        XCTAssertFalse(router.failed)
        XCTAssertTrue(router.fallback)
        XCTAssertEqual(router.takeLegacyReplay(), [], "a cleanup is never re-sent as user input")
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .legacy)
        _ = router.cancelTarget(now: now)
        var again = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { again = $0; return true }
        XCTAssertNotEqual(again, cleanup)
        XCTAssertEqual(Data(base64Encoded: String(again.split(separator: " ")[2])), Data("releaseall:1x2".utf8))
        _ = router.poll(binding: binding, serviceReady: true, lines: receipt(again, count: 1), now: now) { _ in false }
        XCTAssertEqual(router.count, 0)
        XCTAssertFalse(router.failed)
    }

    func testUnsentPressDoesNotProduceCleanupAndHeldSidesRemainIndependent() {
        var router = ready()
        _ = router.route(.pointer("press:0x0"), binding: binding, now: now)
        _ = router.cancelTarget(now: now)
        XCTAssertEqual(router.count, 0)
        var held = HvfPointerHeldState()
        held.sent(.pointer("press:1x2")); held.sent(.pointer("rightclick:3x4"))
        held.inserted(.pointer("rightclick:3x4"))
        XCTAssertEqual(held.release, "releaseall:3x4")
        held.sent(.pointer("releaseall:5x6"))
        XCTAssertEqual(held.release, "releaseall:5x6")
        held.inserted(.pointer("releaseall:5x6"))
        XCTAssertNil(held.release)
    }
}

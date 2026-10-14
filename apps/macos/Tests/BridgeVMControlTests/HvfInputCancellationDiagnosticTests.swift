import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfInputCancellationDiagnosticTests: XCTestCase {
    private let binding = ["disk", "vars", "evidence", "control"]

    func testFailedSendAfterReceiptReportsOnlyRemainingCancelledEntries() throws {
        try withDriver { driver, diagnostics in
            for event in [HvfOrderedInputQueue.Event.text("a"), .text("private fixture"), .key("enter")] {
                XCTAssertEqual(driver.route(event, binding: binding), .queued)
            }
            var first = ""
            driver.poll(binding: binding, serviceReady: true, lines: []) { first = $0; return true }
            let generation = driver.textInputGeneration
            var attempted = 0
            driver.poll(binding: binding, serviceReady: true, lines: receipt(first)) { _ in attempted += 1; return false }
            XCTAssertEqual(attempted, 1)
            XCTAssertEqual(driver.router.count, 0)
            XCTAssertEqual(driver.router.failure, .requestFailed)
            XCTAssertNotEqual(driver.textInputGeneration, generation)
            XCTAssertEqual(diagnostics(), ["ordered input cancelled: transportFailed, discarded=2"])
            driver.poll(binding: binding, serviceReady: true, lines: receipt(first)) { _ in
                XCTFail("Cancelled input must not be replayed"); return true
            }
            XCTAssertEqual(diagnostics().count, 1)
        }
    }

    func testTargetChangeReportsCancelledEntriesOnceAndAdmitsFreshInput() throws {
        for sent in [false, true] {
            try withDriver { driver, diagnostics in
                XCTAssertEqual(driver.route(.text("old target"), binding: binding), .queued)
                XCTAssertEqual(driver.route(.key("enter"), binding: binding), .queued)
                if sent { driver.poll(binding: binding, serviceReady: true, lines: []) { _ in true } }
                driver.cancelTarget()
                XCTAssertEqual(driver.router.count, 0)
                XCTAssertEqual(driver.router.state, .ready)
                XCTAssertEqual(diagnostics(), ["ordered input cancelled: targetChanged, discarded=2"])
                driver.cancelTarget()
                XCTAssertEqual(diagnostics().count, 1, "An empty cancellation is silent")
                XCTAssertEqual(driver.route(.text("new target"), binding: binding), .queued)
            }
        }
    }

    func testSuccessfulContinuationAndEmptyTargetChangeDoNotReportCancellation() throws {
        try withDriver { driver, diagnostics in
            driver.cancelTarget()
            XCTAssertEqual(diagnostics(), [])
            XCTAssertEqual(driver.route(.text("a"), binding: binding), .queued)
            XCTAssertEqual(driver.route(.key("enter"), binding: binding), .queued)
            var first = "", second = ""
            driver.poll(binding: binding, serviceReady: true, lines: []) { first = $0; return true }
            driver.poll(binding: binding, serviceReady: true, lines: receipt(first)) { second = $0; return true }
            XCTAssertTrue(second.hasPrefix("KEYINPUT "))
            XCTAssertEqual(driver.router.count, 1)
            XCTAssertEqual(diagnostics(), [])
            driver.poll(binding: binding, serviceReady: true, lines: receipt(second)) { _ in
                XCTFail("No remaining request should be sent"); return false
            }
            XCTAssertEqual(driver.router.count, 0)
            XCTAssertEqual(diagnostics(), [])
        }
    }

    private func receipt(_ command: String) -> [String] {
        let fields = command.split(separator: " ")
        let label = fields.prefix(2).joined(separator: " ")
        return ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(fields[1]) 2", "BVAGENT END \(label)"]
    }

    private func withDriver(_ body: (HvfSessionInputDriver, () -> [String]) throws -> Void) throws {
        let driver = HvfSessionInputDriver()
        driver.beginOwnedBoot(binding: binding)
        defer { driver.beginOwnedBoot(binding: binding) }
        var command = ""
        driver.poll(binding: binding, serviceReady: true, lines: []) { command = $0; return true }
        let id = try XCTUnwrap(command.split(separator: " ").last)
        driver.poll(binding: binding, serviceReady: true, lines: ["BVAGENT CMD \(command) exit=0",
            "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(command)"]) { _ in false }
        driver.poll(binding: binding, serviceReady: true, lines: []) { _ in false }
        XCTAssertEqual(driver.router.state, .ready)
        var diagnostics: [String] = []
        driver.onDiagnostic = { diagnostics.append($0) }
        try body(driver, { diagnostics })
    }
}

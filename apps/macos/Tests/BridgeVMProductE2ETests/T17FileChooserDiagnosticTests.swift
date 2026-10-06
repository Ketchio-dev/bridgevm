import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserDiagnosticTests: XCTestCase {
    func testTimeoutRetainsDiagnosticContextWithoutAdvancing() {
        let failure = T17FileChooserDiagnosticFixture.timeout("activation=false; key=5; windows=open-panel")
        XCTAssertEqual(failure?.code, "input-selection-failed")
        let message = T17FileChooserDiagnosticEnvelope.inspect(failure)
        XCTAssertEqual(message.original, "stage=location-field-ready; timed out waiting for Go To location field; activation=false; key=5; windows=open-panel")
    }

    func testEmptyContextPreservesOriginalFailureText() {
        let message = T17FileChooserDiagnosticEnvelope.inspect(T17FileChooserDiagnosticFixture.timeout(""))
        XCTAssertEqual(message.original, "stage=location-field-ready; timed out waiting for Go To location field")
    }

    func testDiagnosticContextIsBounded() {
        let prefix = "stage=location-field-ready; timed out waiting for Go To location field; "
        let failure = T17FileChooserDiagnosticFixture.timeout(String(repeating: "x", count: 2_000))
        let message = T17FileChooserDiagnosticEnvelope.inspect(failure)
        XCTAssertEqual(message.original.count, prefix.count + 900)
        XCTAssertEqual(message.original, prefix + String(repeating: "x", count: 900))
        for count in [899, 900, 901] {
            let boundary = T17FileChooserDiagnosticEnvelope.inspect(T17FileChooserDiagnosticFixture.timeout(String(repeating: "x", count: count)))
            XCTAssertEqual(boundary.original, prefix + String(repeating: "x", count: min(count, 900)))
        }
    }

    func testOnlyKnownRolesAndIdentifiersAreDisclosed() {
        for value in ["open-panel", "GoToWindow", "PathTextField", "AXWindow", "AXSheet"] {
            XCTAssertEqual(T17FileChooserDiagnostics.label(value), value)
        }
        for value in ["/private/user/document.iso", "Personal window title", "open-panel-secret"] {
            XCTAssertEqual(T17FileChooserDiagnostics.label(value), "other")
        }
        XCTAssertEqual(T17FileChooserDiagnostics.label(nil), "none")
    }
}

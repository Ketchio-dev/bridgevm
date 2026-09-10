import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserDiagnosticTests: XCTestCase {
    private func timeout(_ context: String) -> T17Blocker? {
        var clock: TimeInterval = 0
        do {
            try T17FileChooser.choose(path: "/private/input.iso", timeout: 1, driver: Driver(context),
                                      now: { clock }, pause: { clock += 1 })
            XCTFail("A missing location field must not succeed")
        } catch let blocker as T17Blocker { return blocker }
        catch { XCTFail("Unexpected error: \(error)") }
        return nil
    }

    func testTimeoutRetainsDiagnosticContextWithoutAdvancing() {
        let failure = timeout("activation=false; key=5; windows=open-panel")
        XCTAssertEqual(failure?.code, "input-selection-failed")
        XCTAssertEqual(failure?.detail, "timed out waiting for Go To location field; activation=false; key=5; windows=open-panel")
    }

    func testEmptyContextPreservesOriginalFailureText() {
        XCTAssertEqual(timeout("")?.detail, "timed out waiting for Go To location field")
    }

    func testDiagnosticContextIsBounded() {
        let failure = timeout(String(repeating: "x", count: 2_000))
        XCTAssertEqual(failure?.detail.count, "timed out waiting for Go To location field; ".count + 900)
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

    private final class Driver: T17FileChooserDriving {
        let failureContext: String
        private var opened = false
        init(_ context: String) { failureContext = context }
        func open() { opened = true }
        func panelIsPresent() -> Bool { opened }
        func showLocationField() {}
        func locationFieldIsReady() -> Bool { false }
        func setLocation(_ path: String) { XCTFail("Must not type without a field") }
        func acceptLocation() { XCTFail("Must not confirm without a field") }
        func locationFieldIsAbsent() -> Bool { false }
        func selectionIsReady() -> Bool { false }
        func acceptSelection() { XCTFail("Must not accept without a field") }
        func selectedPath() -> String? { nil }
    }
}

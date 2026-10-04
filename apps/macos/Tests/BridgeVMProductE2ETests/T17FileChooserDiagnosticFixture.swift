import XCTest
@testable import BridgeVMProductE2E

enum T17FileChooserDiagnosticFixture {
    static func timeout(_ context: String) -> T17Blocker? {
        var clock: TimeInterval = 0
        do {
            try T17FileChooser.choose(path: "/private/input.iso", timeout: 1, driver: Driver(context),
                                      now: { clock }, pause: { clock += 1 })
            XCTFail("A missing location field must not succeed")
        } catch let blocker as T17Blocker { return blocker }
        catch { XCTFail("Unexpected error: \(error)") }
        return nil
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

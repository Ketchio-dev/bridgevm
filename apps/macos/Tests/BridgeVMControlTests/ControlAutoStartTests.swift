import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlAutoStartTests: XCTestCase {
    func testSelectionAndOptionKeySkip() throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        var on = try XCTUnwrap(f.library.vms.first), off = on, installing = on
        on.startWithBridgeVM = true
        off.startWithBridgeVM = false
        installing.startWithBridgeVM = true; installing.installPending = true
        XCTAssertEqual(ControlAutoStart.selection([off, on, installing], optionHeld: false), [on])
        XCTAssertEqual(ControlAutoStart.selection([on], optionHeld: true), [])
    }
    func testSequentialOrderingAndFailureDoesNotSkipNextVM() async throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        var first = try XCTUnwrap(f.library.vms.first), second = first
        first.startWithBridgeVM = true; second.startWithBridgeVM = true
        first.id = "first"; second.id = "second"
        var events: [String] = []
        await ControlAutoStart.run([first, second], optionHeld: false, start: { config in
            events.append("begin-" + config.slug)
            await Task.yield()
            if config.slug == "first" { throw CocoaError(.fileReadUnknown) }
            events.append("end-" + config.slug)
        }, failed: { config, _ in events.append("failed-" + config.slug) })
        XCTAssertEqual(events, ["begin-first", "failed-first", "begin-second", "end-second"])
    }
    func testRegistrationRoundTripAndMissingAndUnknownFields() throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        var config = try XCTUnwrap(f.library.vms.first)
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(config)) as? [String: Any])
        object["futurePreference"] = true
        XCTAssertNil(try JSONDecoder().decode(VMConfig.self, from: JSONSerialization.data(withJSONObject: object)).startWithBridgeVM)
        config.startWithBridgeVM = true
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: JSONEncoder().encode(config)).startWithBridgeVM, true)
    }
}

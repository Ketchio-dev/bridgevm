import XCTest
@testable import BridgeVMProductE2E

final class A9ImportFormEntryTests: XCTestCase {
    func testRequestedNameReachesTheImportBinding() throws {
        let fixture = try A9CleanupFixture(); defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        let ui = FirstRunImportFormUI()
        try A9ImportFormEntry.enter(fixture.request, ui: ui)
        XCTAssertEqual(ui.committedNames, [fixture.request.vmName])
    }

    func testNameThatDoesNotPersistStopsBeforeCommit() throws {
        let fixture = try A9CleanupFixture(); defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        let ui = FirstRunImportFormUI(); ui.focusBinds = false
        XCTAssertThrowsError(try A9ImportFormEntry.enter(fixture.request, ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "input-selection-failed")
        }
        XCTAssertEqual(ui.committedNames, [])
    }

    func testEveryInputIsChosenBeforeCommit() throws {
        let fixture = try A9CleanupFixture(); defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        let ui = FirstRunImportFormUI()
        try A9ImportFormEntry.enter(fixture.request, ui: ui)
        XCTAssertEqual(ui.calls, ["press:bridgevm.first-run.import"]
            + ["disk", "vars", "vtpm", "vtpm-package", "vtpm-code"].map { "choose:bridgevm.first-run.\($0).choose" }
            + ["press:bridgevm.first-run.import.commit"])
    }
}

/// The first-run import form as SwiftUI behaves: the name field has no focus when the form
/// opens, a value set through accessibility reaches the binding only while the field has
/// focus, and the next view update shows the binding again. T19 r1 imported under the
/// default name (vTPM key account `windows-11`, 2026-10-22).
private final class FirstRunImportFormUI: T17UIControlling {
    var binding = "Windows 11", shown = "Windows 11", focused: String?, open = false, focusBinds = true
    var calls: [String] = [], committedNames: [String] = []
    private func missing(_ id: String) -> T17Blocker { T17Blocker(code: "ui-element-missing", detail: id) }
    private func field(_ id: String) throws { guard open, id == "bridgevm.first-run.name" else { throw missing(id) } }
    func press(_ identifier: String, timeout: TimeInterval) throws {
        calls.append("press:\(identifier)")
        switch identifier {
        case "bridgevm.first-run.import": open = true
        case "bridgevm.first-run.import.commit" where open: committedNames.append(binding)
        default: throw missing(identifier)
        }
    }
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws {
        try field(identifier); shown = value
        if focused == identifier, focusBinds { binding = value }
    }
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws {
        try field(identifier); focused = identifier; shown = value
        if focusBinds { binding = value }
    }
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {
        guard open, identifier.hasPrefix("bridgevm.first-run."), identifier.hasSuffix(".choose") else { throw missing(identifier) }
        calls.append("choose:\(identifier)"); focused = nil; shown = binding
    }
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { try field(identifier); return shown }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func expand(_ identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval) throws { throw missing("display") }
    func textSnapshot() -> [String] { [] }
}

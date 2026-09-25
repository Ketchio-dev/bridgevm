import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserSelectionTargetTests: XCTestCase {
    func testEveryRegisteredButtonReadsItsActualProductField() throws {
        let expected: [(String, String, String?)] = [
            ("bridgevm.create.windows.iso", "bridgevm.create.windows.iso.selection", nil),
            ("bridgevm.create.windows.guest-payload", "bridgevm.create.windows.guest-payload.selection", nil),
            ("bridgevm.create.windows.guest-manifest", "bridgevm.create.windows.guest-manifest.selection", nil),
            ("bridgevm.runtime.share.host.choose", "bridgevm.runtime.share.host", "AXTextField"),
            ("bridgevm.first-run.disk.choose", "bridgevm.first-run.disk.path", "AXTextField"),
            ("bridgevm.first-run.vars.choose", "bridgevm.first-run.vars.path", "AXTextField"),
            ("bridgevm.first-run.vtpm.choose", "bridgevm.first-run.vtpm.path", "AXTextField"),
            ("bridgevm.first-run.vtpm-package.choose", "bridgevm.first-run.vtpm-package.path", "AXTextField"),
            ("bridgevm.first-run.vtpm-code.choose", "bridgevm.first-run.vtpm-code.path", "AXTextField"),
        ]
        for (button, identifier, role) in expected {
            let target = try T17ChooserSelectionTarget.resolve(button: button)
            XCTAssertEqual(target.identifier, identifier)
            XCTAssertEqual(target.requiredRole, role)
        }
    }

    func testUnregisteredButtonFailsClosed() {
        XCTAssertThrowsError(try T17ChooserSelectionTarget.resolve(button: "unknown.choose")) {
            XCTAssertEqual($0 as? T17Blocker, T17FileChooser.failure("unregistered chooser result target"))
        }
    }

    func testTextFieldRoleSkipsUnrelatedIdentifierRead() throws {
        let target = try T17ChooserSelectionTarget.resolve(button: "bridgevm.runtime.share.host.choose")
        var identifierReads: [Int] = []
        let selected = try target.read(in: [0, 1], role: { $0 == 0 ? "AXButton" : "AXTextField" },
            identifier: { node in
                identifierReads.append(node)
                if node == 0 { throw T17FileChooser.failure("unrelated identifier read") }
                return "bridgevm.runtime.share.host"
            }, value: { _ in "/fixture/share" }, same: ==)
        XCTAssertEqual(selected, "/fixture/share")
        XCTAssertEqual(identifierReads, [1])
    }

    func testWrongRoleAndMissingIdentifierAreNotSelection() throws {
        let target = try T17ChooserSelectionTarget.resolve(button: "bridgevm.first-run.disk.choose")
        XCTAssertNil(try target.read(in: [0], role: { _ in "AXButton" },
            identifier: { _ in throw T17FileChooser.failure("should not read") },
            value: { _ in "/fixture/disk" }, same: ==))
        XCTAssertNil(try target.read(in: [0], role: { _ in "AXTextField" },
            identifier: { _ in "other.path" }, value: { _ in "/fixture/disk" }, same: ==))
    }

    func testDistinctMatchingFieldsAreRejected() throws {
        let target = try T17ChooserSelectionTarget.resolve(button: "bridgevm.first-run.vars.choose")
        XCTAssertThrowsError(try target.read(in: [0, 1], role: { _ in "AXTextField" },
            identifier: { _ in "bridgevm.first-run.vars.path" },
            value: { _ in "/fixture/vars" }, same: ==)) {
            XCTAssertEqual($0 as? T17Blocker, T17FileChooser.failure("identified chooser element was ambiguous"))
        }
    }

    func testTargetReadFailuresPropagate() throws {
        let target = try T17ChooserSelectionTarget.resolve(button: "bridgevm.runtime.share.host.choose")
        let identifierFailure = T17FileChooser.failure("target identifier read failed")
        XCTAssertThrowsError(try target.read(in: [0], role: { _ in "AXTextField" },
            identifier: { _ in throw identifierFailure }, value: { _ in "/fixture/share" }, same: ==)) {
            XCTAssertEqual($0 as? T17Blocker, identifierFailure)
        }
        let valueFailure = T17FileChooser.failure("target value read failed")
        XCTAssertThrowsError(try target.read(in: [0], role: { _ in "AXTextField" },
            identifier: { _ in "bridgevm.runtime.share.host" }, value: { _ in throw valueFailure }, same: ==)) {
            XCTAssertEqual($0 as? T17Blocker, valueFailure)
        }
    }

    func testCreateScreenSelectionLabelRetainsExistingLookup() throws {
        let target = try T17ChooserSelectionTarget.resolve(button: "bridgevm.create.windows.iso")
        let selected = try target.read(in: [0, 1], role: { _ in throw T17FileChooser.failure("no role required") },
            identifier: { $0 == 0 ? "other" : "bridgevm.create.windows.iso.selection" },
            value: { _ in "/fixture/install.iso" }, same: ==)
        XCTAssertEqual(selected, "/fixture/install.iso")
    }
}

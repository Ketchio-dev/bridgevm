import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

/// Drives the composition element() uses (snapshot, projection, poll) over scripted
/// nodes. 0.25 s per snapshot retry and 0.125 s per poll pause are binary-exact
/// stand-ins for the production 0.2 s and 0.1 s.
final class T17IdentifierSearchTests: XCTestCase {
    private static let input = "bridgevm.runtime.ctl.input"
    private static let application = 0, group = 1, field = 2

    private final class Harness {
        let start = Date(timeIntervalSinceReferenceDate: 0)
        lazy var clock = start
        var reads = 0, captures = 0
        var elapsed: TimeInterval { clock.timeIntervalSince(start) }

        func run(role: String?, timeout: TimeInterval, nodes: (Int) throws -> [Int] = { _ in [application, group, field] },
                 attribute: (Int, String) throws -> String?) throws -> Int {
            try T17IdentifierSearch.find(input, role: role, timeout: timeout, now: { self.clock },
                pause: { self.clock += 0.125 }, snapshotPause: { self.clock += 0.25 },
                root: { self.reads += 1; return application }, nodes: { _ in try nodes(self.reads) },
                attribute: attribute, missing: {
                    self.captures += 1
                    return T17Blocker(code: "ui-element-missing", detail: "missing-capture")
                })
        }
    }

    private static func tree(_ node: Int, _ name: String) -> String? {
        switch (node, name) {
        case (field, kAXIdentifierAttribute): return input
        case (field, kAXRoleAttribute): return kAXTextFieldRole
        case (group, kAXRoleAttribute): return kAXGroupRole
        default: return nil
        }
    }

    private static func readFailure(_ attribute: String, _ status: AXError) -> T17Blocker {
        T17Blocker(code: "ui-element-missing",
                   detail: "ax_tree_read_failed;attribute=\(attribute);role=AXTextField;ax_error=\(status.rawValue)")
    }

    /// r37 shape: setText's role-qualified lookup of ctl.input, AXIdentifier -25200 on the field.
    func testTransientTargetReadsRecoverWithinTheRoleQualifiedDeadline() {
        let harness = Harness()
        XCTAssertEqual(try harness.run(role: kAXTextFieldRole, timeout: 10) { node, name in
            if node == Self.field, name == kAXIdentifierAttribute, harness.reads <= 6 {
                throw Self.readFailure(name, harness.reads <= 3 ? .failure : .invalidUIElement)
            }
            return Self.tree(node, name)
        }, Self.field)
        XCTAssertEqual(harness.reads, 7)
        XCTAssertEqual(harness.elapsed, 1.25)
        XCTAssertEqual(harness.captures, 0)
    }

    func testTransientTreeWalkRecoversWithinTheUnqualifiedDeadline() {
        let harness = Harness()
        XCTAssertEqual(try harness.run(role: nil, timeout: 10, nodes: { read in
            if read <= 3 { throw Self.readFailure(kAXChildrenAttribute, .invalidUIElement) }
            return [Self.application, Self.group, Self.field]
        }, attribute: Self.tree), Self.field)
        XCTAssertEqual(harness.reads, 4)
        XCTAssertEqual(harness.elapsed, 0.625)
        XCTAssertEqual(harness.captures, 0)
    }

    func testPersistentTransientReadIsThrownOnceAttributedAtTheDeadline() {
        let harness = Harness()
        XCTAssertThrowsError(try harness.run(role: kAXTextFieldRole, timeout: 2.5) { node, name in
            if node == Self.field, name == kAXIdentifierAttribute { throw Self.readFailure(name, .failure) }
            return Self.tree(node, name)
        }) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "ax_tree_read_failed;attribute=AXIdentifier;role=AXTextField;"
                           + "ax_error=-25200;stage=identifier-search;identifier=\(Self.input)")
        }
        XCTAssertEqual(harness.reads, 12)
        XCTAssertEqual(harness.elapsed, 2.5)
        XCTAssertEqual(harness.captures, 0)
    }

    func testAmbiguityFailsAtOnceAndRoleMismatchIsAbsenceUntilTheDeadline() {
        let ambiguous = Harness()
        XCTAssertThrowsError(try ambiguous.run(role: kAXTextFieldRole, timeout: 10,
                                               nodes: { _ in [Self.field, Self.field] }, attribute: Self.tree)) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "ambiguous accessibility identity: \(Self.input);"
                           + "role=AXTextField;matches=2;stage=identifier-search;identifier=\(Self.input)")
        }
        XCTAssertEqual(ambiguous.reads, 1)
        let mismatch = Harness()
        XCTAssertThrowsError(try mismatch.run(role: kAXButtonRole, timeout: 0.5, attribute: Self.tree)) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "missing-capture")
        }
        XCTAssertEqual(mismatch.reads, 4)
        XCTAssertEqual(mismatch.captures, 1)
    }
}

import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserTreeDiagnosticTests: XCTestCase {
    func testRecordsNestedLocationFieldWithoutPrivateValues() {
        let result = T17FileChooserTreeDiagnostics.summarize(roots: [0], metadata: {
            [("AXWindow", "open-panel"), ("AXSheet", "GoToWindow"),
             ("AXTextField", "PathTextField"), ("Secret title", "/private/input.iso")][$0]
        }, children: { $0 == 0 ? [1, 3] : $0 == 1 ? [2] : [] })
        XCTAssertTrue(result.contains("nodes=4,limited=false,errors=0,matches=3"))
        XCTAssertTrue(result.contains("AXSheet/GoToWindow"))
        XCTAssertTrue(result.contains("AXTextField/PathTextField"))
        XCTAssertFalse(result.contains("Secret"))
        XCTAssertFalse(result.contains("private"))
    }

    func testCyclesAndWideTreesAreBounded() {
        var visits = 0
        let result = T17FileChooserTreeDiagnostics.summarize(roots: [0], metadata: { _ in
            visits += 1
            return ("AXWindow", nil)
        }, children: { _ in Array(repeating: 0, count: 200) })
        XCTAssertEqual(visits, 128)
        XCTAssertTrue(result.contains("nodes=128,limited=true"))
        XCTAssertLessThan(result.count, 300)
    }

    func testReadFailuresAreNotReportedAsAnEmptySuccessfulTree() {
        enum Failure: Error { case unreadable }
        let result = T17FileChooserTreeDiagnostics.summarize(roots: [0], metadata: { _ in
            throw Failure.unreadable
        }, children: { _ in throw Failure.unreadable })
        XCTAssertEqual(result, "nodes=1,limited=false,errors=2,matches=0[]")
    }

    func testOversizedRootInventoryReportsTruncation() {
        let result = T17FileChooserTreeDiagnostics.summarize(roots: Array(0..<200),
            metadata: { _ in (nil, nil) }, children: { _ in [] })
        XCTAssertEqual(result, "nodes=128,limited=true,errors=0,matches=0[]")
    }
}

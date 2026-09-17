import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class ProductE2ECLITests: XCTestCase {
    func testImportModeIsExplicitAndDistinct() throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
        let result = directory.appendingPathComponent("bridgevm-import-result-\(UUID().uuidString).json")
        let parsed = try ProductE2ECLI.parse([
            "--windows-import-product-e2e", "--request", "/tmp/import-request.json",
            "--result", result.path,
        ])
        XCTAssertEqual(parsed.mode, .installedDiskImport)
    }

    func testMixedOrDuplicateModesFailClosed() throws {
        let result = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("bridgevm-import-result-\(UUID().uuidString).json")
        XCTAssertThrowsError(try ProductE2ECLI.parse([
            "--windows-product-e2e", "--windows-import-product-e2e",
            "--request", "/tmp/request.json", "--result", result.path,
        ]))
        XCTAssertThrowsError(try ProductE2ECLI.parse([
            "--windows-import-product-e2e", "--windows-import-product-e2e",
            "--request", "/tmp/request.json", "--result", result.path,
        ]))
    }
}

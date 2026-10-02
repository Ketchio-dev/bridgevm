import XCTest
@testable import BridgeVMProductE2E

final class T17SelectedMediaTests: XCTestCase {
    private let a = String(repeating: "a", count: 64), b = String(repeating: "b", count: 64)
    private var output: String { "disk_bytes 10\ndisk_sha256 \(a)\nvars_bytes 4\nvars_sha256 \(b)\n" }

    func testTheHelperIsResolvedInsideTheProductAppResources() {
        let helper = URL(fileURLWithPath: "/x/BridgeVM.app/Contents/Helpers/BridgeVMProductE2E.app")
        XCTAssertEqual(T17SelectedMedia.cli(helperBundle: helper).path,
                       "/x/BridgeVM.app/Contents/Resources/target/release/examples/snapshot_pair_cli")
    }
    func testDigestRunsTheDigestCommandAndParsesExactlyFourFields() throws {
        var seen: [String] = []
        let digest = try T17SelectedMedia.digest(disk: "/d", vars: "/v") { _, args in seen = args; return self.output }
        XCTAssertEqual(seen, ["digest", "/d", "/v"])
        XCTAssertEqual(digest, .init(diskBytes: 10, diskSHA256: a, varsBytes: 4, varsSHA256: b))
        XCTAssertNil(T17SelectedMedia.parse(output + "extra 1\n"))
        XCTAssertNil(T17SelectedMedia.parse(output.replacingOccurrences(of: a, with: "zz")))
        XCTAssertThrowsError(try T17SelectedMedia.digest(disk: "/d", vars: "/v") { _, _ in "" })
    }
    func testTheDifferenceNamesWhichMediumDiffers() throws {
        let digest = try XCTUnwrap(T17SelectedMedia.parse(output))
        let manifest: [String: Any] = ["disk_bytes": 10, "disk_sha256": a, "vars_bytes": 4, "vars_sha256": b]
        XCTAssertNil(T17SelectedMedia.difference(digest, manifest: manifest))
        var changed = manifest; changed["vars_sha256"] = a
        XCTAssertEqual(T17SelectedMedia.difference(digest, manifest: changed),
                       "restored media differs from the snapshot pair; disk=match vars=differs")
    }
}

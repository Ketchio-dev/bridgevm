import Foundation
import XCTest
@testable import BridgeVMProductE2E
final class A9ImportRequestTests: XCTestCase {
    private typealias Fixture = A9ImportRequestFixture

    func testExactReadOnlyPilotRequestLoads() throws {
        let fixture = try Fixture()
        let request = try A9ImportRequest.load(fixture.requestURL)
        XCTAssertEqual(request.campaignMode, "pilot")
        XCTAssertFalse(request.threeDInjection)
        XCTAssertNotEqual(request.sourceDiskPath, request.diskPath)
    }

    func testUnknownFieldFailsClosed() throws {
        let fixture = try Fixture()
        fixture.body["unexpected"] = true; try fixture.write()
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func test3DEnabledRequestIsRejected() throws {
        let fixture = try Fixture()
        fixture.body["three_d_injection"] = true; try fixture.write()
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testWritableSourceMediaIsRejected() throws {
        let fixture = try Fixture()
        try FileManager.default.setAttributes([.posixPermissions: 0o644],
            ofItemAtPath: fixture.body["source_disk_path"] as! String)
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testWrongVarsSizeIsRejected() throws {
        let fixture = try Fixture()
        let path = fixture.body["source_vars_path"] as! String
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path)
        let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        try handle.truncate(atOffset: 4096); try handle.close()
        try FileManager.default.setAttributes([.posixPermissions: 0o444], ofItemAtPath: path)
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testSymlinkedSourceMediaIsRejected() throws {
        let fixture = try Fixture()
        let target = fixture.root.appendingPathComponent("other.raw")
        try Data([4]).write(to: target)
        try fixture.replaceSourceDisk(with: target)
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testEscapedLanePathIsRejected() throws {
        let fixture = try Fixture()
        fixture.body["share_path"] = "/tmp/outside-share"; try fixture.write()
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }
}

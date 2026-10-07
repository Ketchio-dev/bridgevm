import XCTest
@testable import BridgeVMControl

final class E2EAllocationAdmissionTests: XCTestCase {
    func testInstallAndImportAllocationBoundLibrariesAreAccepted() throws {
        for importing in [false, true] {
            let fixture = try E2EAdmissionFixture(importing: importing)
            let parsed = try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path])
            XCTAssertEqual(parsed.e2eLibraryRoot?.path, fixture.library.path)
        }
    }

    func testStaleIdentityWrongJobAndMissingAllocationRefuse() throws {
        for key in ["work_parent_identity", "work_identity", "job_id", "lane", "work_parent"] {
            let fixture = try E2EAdmissionFixture()
            fixture.body[key] = key == "lane" ? 2 : "wrong"
            try fixture.write()
            XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path]))
        }
        let fixture = try E2EAdmissionFixture()
        fixture.body.removeValue(forKey: "work_identity"); try fixture.write()
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path]))
    }

    func testDuplicateUnknownFieldsAndRequestSymlinkRefuse() throws {
        let fixture = try E2EAdmissionFixture()
        let data = try String(contentsOf: fixture.request, encoding: .utf8)
        try Data(("{\"lane\":1," + data.dropFirst()).utf8).write(to: fixture.request)
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path]))
        fixture.body["unknown"] = true; try fixture.write()
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path]))
        let saved = fixture.lane.appendingPathComponent("saved-request")
        try FileManager.default.moveItem(at: fixture.request, to: saved)
        try FileManager.default.createSymbolicLink(at: fixture.request, withDestinationURL: saved)
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path]))
    }

    func testNonprivateOrSymlinkedAllocationRefuses() throws {
        let fixture = try E2EAdmissionFixture()
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fixture.work.path)
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", fixture.library.path]))
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fixture.work.path)
        let alias = fixture.parent.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.work)
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", alias.appendingPathComponent("lane-1/library").path]))
    }
}

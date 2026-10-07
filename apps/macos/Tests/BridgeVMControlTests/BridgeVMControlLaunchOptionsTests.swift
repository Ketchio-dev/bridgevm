import XCTest
@testable import BridgeVMControl

final class BridgeVMControlLaunchOptionsTests: XCTestCase {
    func testDefaultUsesNoOverride() throws {
        let parsed = try BridgeVMControlLaunchOptions.parse(arguments: [])
        XCTAssertNil(parsed.e2eLibraryRoot)
        XCTAssertNil(parsed.e2eUnattendedPath)
    }

    func testE2EAnswerFileMustBeRegularAndBesideTheIsolatedLibrary() throws {
        let fixture = try E2EAdmissionFixture(), lane = fixture.lane, library = fixture.library
        let answer = lane.appendingPathComponent("e2e-unattend.xml")
        try Data("<unattend/>".utf8).write(to: answer)

        let parsed = try BridgeVMControlLaunchOptions.parse(arguments: [
            "--e2e-unattend-path", answer.path,
            "--e2e-library-root", library.path,
        ])
        XCTAssertEqual(parsed.e2eUnattendedPath?.path, answer.resolvingSymlinksInPath().path)
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-unattend-path", answer.path]))

        let outside = lane.deletingLastPathComponent().appendingPathComponent("outside.xml")
        try Data("<unattend/>".utf8).write(to: outside)
        defer { try? FileManager.default.removeItem(at: outside) }
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: [
            "--e2e-library-root", library.path,
            "--e2e-unattend-path", outside.path,
        ]))
    }

    func testAcceptsOnlyAnExistingEmptyCanonicalE2ERoot() throws {
        let fixture = try E2EAdmissionFixture(), root = fixture.library
        let parsed = try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", root.path])
        XCTAssertEqual(parsed.e2eLibraryRoot?.path, root.resolvingSymlinksInPath().path)
    }

    func testRejectsRelativeDuplicateNonEmptyAndUnrelatedRoots() throws {
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", "relative"]))
        let fixture = try E2EAdmissionFixture()
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: [
            "--e2e-library-root", fixture.library.path, "--e2e-library-root", fixture.library.path,
        ]))
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", FileManager.default.homeDirectoryForCurrentUser.path]))

        let nonEmpty = fixture.library
        try Data("occupied".utf8).write(to: nonEmpty.appendingPathComponent("entry"))
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", nonEmpty.path]))
    }
}

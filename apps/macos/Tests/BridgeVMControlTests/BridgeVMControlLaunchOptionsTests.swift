import XCTest
@testable import BridgeVMControl

final class BridgeVMControlLaunchOptionsTests: XCTestCase {
    func testDefaultUsesNoOverride() throws {
        let parsed = try BridgeVMControlLaunchOptions.parse(arguments: [])
        XCTAssertNil(parsed.e2eLibraryRoot)
        XCTAssertNil(parsed.e2eUnattendedPath)
    }

    func testE2EAnswerFileMustBeRegularAndBesideTheIsolatedLibrary() throws {
        let lane = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("bridgevm-e2e-answer-\(UUID().uuidString)")
        let library = lane.appendingPathComponent("library", isDirectory: true)
        let answer = lane.appendingPathComponent("e2e-unattend.xml")
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        try Data("<unattend/>".utf8).write(to: answer)
        defer { try? FileManager.default.removeItem(at: lane) }

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
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("bridgevm-e2e-options-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let parsed = try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", root.path])
        XCTAssertEqual(parsed.e2eLibraryRoot?.path, root.resolvingSymlinksInPath().path)
    }

    func testRejectsRelativeDuplicateNonEmptyAndUnrelatedRoots() throws {
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", "relative"]))
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(arguments: [
            "--e2e-library-root", "/tmp/bridgevm-e2e-a",
            "--e2e-library-root", "/tmp/bridgevm-e2e-b",
        ]))
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", FileManager.default.homeDirectoryForCurrentUser.path]))

        let nonEmpty = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("bridgevm-e2e-nonempty-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: nonEmpty, withIntermediateDirectories: false)
        try Data("occupied".utf8).write(to: nonEmpty.appendingPathComponent("entry"))
        defer { try? FileManager.default.removeItem(at: nonEmpty) }
        XCTAssertThrowsError(try BridgeVMControlLaunchOptions.parse(
            arguments: ["--e2e-library-root", nonEmpty.path]))
    }
}

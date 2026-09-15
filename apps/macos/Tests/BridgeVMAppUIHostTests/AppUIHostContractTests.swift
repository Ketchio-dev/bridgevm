#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class AppUIHostContractTests: XCTestCase {
    private func fixture() throws -> (root: URL, output: URL) {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-ui-host-contract-" + UUID().uuidString, isDirectory: true)
        let parent = root.appendingPathComponent("app-ui-private", isDirectory: true)
        let output = parent.appendingPathComponent("host-observations", isDirectory: true)
        for directory in [root, parent, output] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                                   attributes: [.posixPermissions: 0o700])
        }
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return (root, output)
    }
    private func arguments(_ output: URL) -> [String] { ["--app-ui-host", "--output", output.path] }
    private func object(_ output: URL, _ name: String) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: output.appendingPathComponent(name))) as? [String: Any])
    }
    func testAcceptsOnlyExactRequestWithoutWritingToOutput() throws {
        let fixture = try fixture()
        XCTAssertEqual(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)), fixture.output)
        for invalid in [[], ["--app-ui-host"], ["--output", fixture.output.path, "--app-ui-host"],
                        arguments(fixture.output) + ["--vtpm-lifecycle"],
                        ["--app-ui-host", "--output", "app-ui-private/host-observations"],
                        ["--app-ui-host", "--output", fixture.output.path + "/../host-observations"]] {
            XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: invalid))
        }
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: fixture.output.path), [])
    }
    func testRejectsNonemptyOrNonprivateOutputWithoutRemovingEvidence() throws {
        let fixture = try fixture()
        let evidence = fixture.output.appendingPathComponent("retained.txt")
        try Data("retained failure".utf8).write(to: evidence)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        XCTAssertEqual(try String(contentsOf: evidence, encoding: .utf8), "retained failure")
        try FileManager.default.removeItem(at: evidence)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fixture.output.path)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fixture.output.path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fixture.output.deletingLastPathComponent().path)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
    }
    func testRejectsSymlinkedParentAndOutput() throws {
        let fixture = try fixture()
        let aliasRoot = fixture.root.appendingPathComponent("alias", isDirectory: true)
        try FileManager.default.createDirectory(at: aliasRoot, withIntermediateDirectories: false)
        let aliasParent = aliasRoot.appendingPathComponent("app-ui-private", isDirectory: true)
        try FileManager.default.createSymbolicLink(at: aliasParent, withDestinationURL: fixture.output.deletingLastPathComponent())
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(aliasParent.appendingPathComponent("host-observations"))))
        let actual = fixture.output.deletingLastPathComponent().appendingPathComponent("real-output", isDirectory: true)
        try FileManager.default.moveItem(at: fixture.output, to: actual)
        try FileManager.default.createSymbolicLink(at: fixture.output, withDestinationURL: actual)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: actual.path), [])
    }
    func testCancellationMarkerRefusesWithoutChangingOutputAdmission() throws {
        let fixture = try fixture()
        let marker = fixture.output.deletingLastPathComponent().appendingPathComponent("cancel.requested")
        XCTAssertNoThrow(try AppUIHost.checkCancellation(output: fixture.output))
        try Data().write(to: marker)
        XCTAssertEqual(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)), fixture.output)
        XCTAssertThrowsError(try AppUIHost.checkCancellation(output: fixture.output))
        try FileManager.default.removeItem(at: marker)
        try FileManager.default.createSymbolicLink(at: marker,
            withDestinationURL: fixture.root.appendingPathComponent("missing-cancel-target"))
        XCTAssertThrowsError(try AppUIHost.checkCancellation(output: fixture.output))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: fixture.output.path), [])
    }
    func testFixtureConstructionHasNoDomainWorkOrLibraryWrites() throws {
        let fixture = try fixture()
        let capture = try AppUIHostCapture(output: fixture.output)
        let libraryRoot = fixture.output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: libraryRoot, withIntermediateDirectories: false)
        let library = AppUIHost.makeLibrary(root: libraryRoot, capture: capture)
        XCTAssertTrue(library.vms.isEmpty)
        XCTAssertNil(library.selectedID)
        XCTAssertFalse(library.showingCreate)
        XCTAssertEqual(library.rootURL, libraryRoot)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: libraryRoot.path), [])
        let report = try object(fixture.output, "ui-observations.json")
        XCTAssertEqual(report["tripwires"] as? [String: Int], ["model_creations": 0, "runtime_creations": 0,
            "install_creations": 0, "file_jobs": 0])
    }
    func testIncompleteObservationCannotBecomeSuccessfulCompletion() throws {
        let fixture = try fixture()
        let capture = try AppUIHostCapture(output: fixture.output)
        try capture.writeCompletion(cleanupVerified: true)
        let completion = try object(fixture.output, "host-completion.json")
        XCTAssertEqual(completion["success"] as? Bool, false)
        XCTAssertNotNil(completion["failure"] as? String)
        XCTAssertEqual(completion["report_sha256"] as? String,
                       try AppUIHostCapture.digest(fixture.output.appendingPathComponent("ui-observations.json")))
        XCTAssertThrowsError(try capture.writeCompletion(cleanupVerified: true))
    }
    func testUnpackagedTestProcessRefusesBeforeAppOrFixtureCreation() throws {
        let fixture = try fixture()
        XCTAssertNotEqual(Bundle.main.bundleIdentifier, "dev.bridgevm.app-ui-host")
        XCTAssertNil(AppUIHost.prepared)
        XCTAssertThrowsError(try AppUIHost.prepare(arguments: arguments(fixture.output)))
        XCTAssertNil(AppUIHost.prepared)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.output.appendingPathComponent("fixture-library").path))
        let report = try object(fixture.output, "ui-observations.json")
        XCTAssertNotNil(report["failure"] as? String)
        XCTAssertEqual(try object(fixture.output, "host-completion.json")["success"] as? Bool, false)
    }
}
#endif

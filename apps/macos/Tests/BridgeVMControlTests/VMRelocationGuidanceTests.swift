import Foundation
import XCTest
@testable import BridgeVMControl

final class VMRelocationGuidanceTests: XCTestCase {
    func testLibraryIssueShowsRecordedPathsButDoesNotGuessFromMalformedRecord() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? fm.removeItem(at: root) }
        let library = root.appendingPathComponent("library")
        let source = root.appendingPathComponent("source/bundle.vmbridge")
        try fm.createDirectory(at: source, withIntermediateDirectories: true)
        let original = VMConfig(id: "guidance-fixture", name: "Guidance Fixture",
            displayName: "Guidance Fixture", backendKind: "hvf-engine",
            bundlePath: source.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "Windows", displayWidth: 800, displayHeight: 600)
        XCTAssertTrue(VMLibrary.save(original, rootURL: library))
        var moved = original
        moved.bundlePath = root.appendingPathComponent("destination/bundle.vmbridge").path
        let pending = try VMRelocationJournal.begin(original: original, moved: moved, rootURL: library)
        let scan = VMLibrary.scan(rootURL: library)
        XCTAssertTrue(scan.configs.isEmpty)
        let message = try XCTUnwrap(scan.issues.first).message
        XCTAssertTrue(message.contains(original.bundlePath))
        XCTAssertTrue(message.contains(moved.bundlePath))
        XCTAssertFalse(fm.fileExists(atPath: moved.bundlePath))
        try Data("incomplete-record".utf8).write(to: pending)
        let malformed = VMLibrary.scan(rootURL: library)
        XCTAssertTrue(malformed.configs.isEmpty)
        let fallback = try XCTUnwrap(malformed.issues.first).message
        XCTAssertFalse(fallback.contains(original.bundlePath))
        XCTAssertFalse(fallback.contains(moved.bundlePath))
        XCTAssertTrue(fm.fileExists(atPath: pending.path))
        XCTAssertTrue(fm.fileExists(atPath: source.path))
    }
}

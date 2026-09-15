import XCTest
@testable import BridgeVMControl

final class VMRegistrationCommitTests: XCTestCase {
    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }

    func testCommittedReplacementHasOnlyPrivateFinalFile() throws {
        let root = try directory(), url = root.appendingPathComponent("vm.json")
        try Data([1]).write(to: url)
        let outcome = VMRegistrationWriter.commit(Data([2]), to: url)
        guard case .committed = outcome else { return XCTFail("expected committed") }
        XCTAssertTrue(outcome.isCommitted)
        XCTAssertEqual(try Data(contentsOf: url), Data([2]))
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["vm.json"])
    }

    func testRenameRefusalNeverCallsDirectorySyncOrRemovesDestination() throws {
        let root = try directory(), url = root.appendingPathComponent("vm.json")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        try Data([1]).write(to: url.appendingPathComponent("marker"))
        var synced = false
        let outcome = VMRegistrationWriter.commit(Data([2]), to: url, syncParent: { _ in synced = true })
        guard case .notPublished = outcome else { return XCTFail("expected not published") }
        XCTAssertFalse(outcome.isCommitted)
        XCTAssertFalse(synced)
        XCTAssertEqual(try Data(contentsOf: url.appendingPathComponent("marker")), Data([1]))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["vm.json"])
    }

    func testDirectorySyncFailureReportsPublishedAndRetainsReplacement() throws {
        let root = try directory(), url = root.appendingPathComponent("vm.json")
        try Data([1]).write(to: url)
        var synced = false
        let outcome = VMRegistrationWriter.commit(Data([2]), to: url) { parent in
            synced = true
            XCTAssertEqual(parent.path, root.path)
            XCTAssertEqual(try Data(contentsOf: url), Data([2]))
            throw CocoaError(.fileWriteUnknown)
        }
        guard case .publishedButUnsynced = outcome else { return XCTFail("expected uncertain publication") }
        XCTAssertFalse(outcome.isCommitted)
        XCTAssertTrue(synced)
        XCTAssertEqual(try Data(contentsOf: url), Data([2]))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["vm.json"])
    }
}

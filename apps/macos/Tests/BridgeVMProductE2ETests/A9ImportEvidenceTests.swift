import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class A9ImportEvidenceTests: XCTestCase {
    func testEvidenceCannotSkipImportStagesAndSentinelsAreNonceBound() throws {
        let first = String(repeating: "1", count: 64)
        let second = String(repeating: "2", count: 64)
        var evidence = A9ImportEvidence(nonce: first)
        XCTAssertThrowsError(try evidence.prove(.uiImported))
        XCTAssertFalse(evidence.stages[.uiImported]!)
        XCTAssertNotEqual(evidence.hashes["source_disk_sha256"],
                          A9ImportEvidence(nonce: second).hashes["source_disk_sha256"])
        try evidence.prove(.artifactPreflight)
        try evidence.prove(.sourceAuthenticated)
        XCTAssertTrue(evidence.stages[.sourceAuthenticated]!)
    }

    func testTreeDigestIsStableAndIgnoresOnlyTheRuntimeLock() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("bridgevm-a9-tree-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data([1, 2, 3]).write(to: root.appendingPathComponent("state.bin"))
        let before = try A9ImportTreeDigest.compute(root)
        try Data([9]).write(to: root.appendingPathComponent(".lock"))
        XCTAssertEqual(try A9ImportTreeDigest.compute(root), before)
        try Data([4]).write(to: root.appendingPathComponent("state.bin"))
        XCTAssertNotEqual(try A9ImportTreeDigest.compute(root), before)
    }

    func testTreeDigestRejectsSymlinks() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("bridgevm-a9-tree-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let target = root.appendingPathComponent("target")
        try Data([1]).write(to: target)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("link"), withDestinationURL: target)
        XCTAssertThrowsError(try A9ImportTreeDigest.compute(root))
    }
}

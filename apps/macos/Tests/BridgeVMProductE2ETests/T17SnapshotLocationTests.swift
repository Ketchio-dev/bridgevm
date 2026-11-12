import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class T17SnapshotLocationTests: XCTestCase {
    func testLegacyRequestLocatorFindsTimestampedSnapshotAndIgnoresStaging() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("t17-snapshots-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let legacy = root.appendingPathComponent("latest.snapshot")
        for (name, time) in [("latest.snapshot", 100), ("20261112-120000.snapshot", 200), ("20261112-130000.snapshot.staging", 300)] {
            let directory = root.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let manifest = directory.appendingPathComponent("manifest.json")
            try Data("{}".utf8).write(to: manifest)
            try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: Double(time))], ofItemAtPath: manifest.path)
        }
        XCTAssertEqual(try T17SnapshotLocation.resolve(legacy).lastPathComponent, "20261112-120000.snapshot")
        try FileManager.default.removeItem(at: root.appendingPathComponent("20261112-120000.snapshot"))
        XCTAssertEqual(try T17SnapshotLocation.resolve(legacy).path, legacy.standardizedFileURL.path)
    }
}

import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsSnapshotStoreTests: XCTestCase {
    typealias Store = HvfWindowsSnapshotStore

    func testListsNewestFirstIncludingUntouchedLegacySnapshotAndIgnoresStaging() throws {
        let bundle = temporaryBundle(); defer { try? FileManager.default.removeItem(at: bundle) }
        let legacy = try entry(bundle, "latest.snapshot", date: 100)
        let newest = try entry(bundle, "20261112-120000.snapshot", date: 200)
        _ = try entry(bundle, "20261112-110000.snapshot", date: 300)
        _ = try entry(bundle, "20261112-130000.snapshot.staging", date: 400)
        let entries = Store.list(bundle: bundle)
        XCTAssertEqual(entries.map(\.date), [Store.timestamp("20261112-120000.snapshot")!,
                                            Store.timestamp("20261112-110000.snapshot")!, Date(timeIntervalSince1970: 100)])
        XCTAssertEqual(entries.first?.url.path, newest.standardizedFileURL.path)
        XCTAssertEqual(entries.last?.url.path, legacy.standardizedFileURL.path)
        XCTAssertTrue(FileManager.default.fileExists(atPath: legacy.path))
    }

    func testNameIsDisplayOnlyAndTwoCreatesInOneSecondHaveDistinctUTCPaths() throws {
        let bundle = temporaryBundle(); defer { try? FileManager.default.removeItem(at: bundle) }
        let now = Date(timeIntervalSince1970: 0)
        let first = Store.nextURL(bundle: bundle, now: now, occupied: [])
        let second = Store.nextURL(bundle: bundle, now: now, occupied: [first.lastPathComponent])
        XCTAssertEqual(first.lastPathComponent, "19700101-000000.snapshot")
        XCTAssertEqual(second.lastPathComponent, "19700101-000001.snapshot")
        let orphan = Store.nextURL(bundle: bundle, now: now, occupied: [first.lastPathComponent + ".name"])
        XCTAssertEqual(orphan.lastPathComponent, "19700101-000001.snapshot", "never inherit an orphaned display name")
        let staged = Store.nextURL(bundle: bundle, now: now, occupied: [".\(first.lastPathComponent).staging"])
        XCTAssertEqual(staged.lastPathComponent, "19700101-000001.snapshot")
        let snapshot = try entry(bundle, first.lastPathComponent, date: 100)
        let name = "../../a/b..." + String(repeating: "긴 이름/", count: 10_000)
        try name.write(to: Store.nameFile(snapshot), atomically: true, encoding: .utf8)
        XCTAssertEqual(Store.list(bundle: bundle).first?.name, name)
        XCTAssertEqual(Store.list(bundle: bundle).first?.url.path, first.standardizedFileURL.path)
    }

    @MainActor
    func testSnapshotsInTheSameMinuteHaveDistinctRestoreTargets() throws {
        let bundle = temporaryBundle()
        defer { try? FileManager.default.removeItem(at: bundle) }
        for filename in ["20261126-120008.snapshot", "20261126-120034.snapshot"] {
            let snapshot = try entry(bundle, filename, date: 0)
            try "업데이트 전".write(to: Store.nameFile(snapshot), atomically: true, encoding: .utf8)
        }
        let entries = Store.list(bundle: bundle)
        XCTAssertEqual(entries.count, 2)
        XCTAssertEqual(Set(entries.map(\.label)).count, 2)
        let model = HvfWindowsSnapshotModel()
        let questions = try entries.map { entry in
            try XCTUnwrap(model.confirmation(for: .restore, snapshotDate: entry.date, label: entry.label) { _ in nil }).title
        }
        XCTAssertEqual(Set(questions).count, 2)
    }

    func testDeleteRefusesSymlinkOutsideEntryAndSymlinkedParentWithoutTouchingData() throws {
        let bundle = temporaryBundle(); defer { try? FileManager.default.removeItem(at: bundle) }
        let safe = try entry(bundle, "latest.snapshot", date: 100)
        let outside = bundle.appendingPathComponent("outside.snapshot")
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        let link = safe.deletingLastPathComponent().appendingPathComponent("link.snapshot")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside)
        XCTAssertThrowsError(try Store.delete(link, bundle: bundle))
        XCTAssertThrowsError(try Store.delete(outside, bundle: bundle))
        XCTAssertTrue(FileManager.default.fileExists(atPath: outside.path))
        try FileManager.default.removeItem(at: link)
        let directory = safe.deletingLastPathComponent()
        let moved = bundle.appendingPathComponent("moved")
        try FileManager.default.moveItem(at: directory, to: moved)
        try FileManager.default.createSymbolicLink(at: directory, withDestinationURL: moved)
        XCTAssertThrowsError(try Store.delete(safe, bundle: bundle))
        XCTAssertTrue(FileManager.default.fileExists(atPath: moved.appendingPathComponent("latest.snapshot").path))
    }

    private func temporaryBundle() -> URL {
        FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("snapshot-store-\(UUID())")
    }

    private func entry(_ bundle: URL, _ name: String, date: TimeInterval) throws -> URL {
        let url = Store.directory(bundle: bundle).appendingPathComponent(name)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        let manifest = url.appendingPathComponent("manifest.json")
        try Data("{}".utf8).write(to: manifest)
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: date)], ofItemAtPath: manifest.path)
        return url
    }
}

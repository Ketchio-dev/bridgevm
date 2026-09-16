import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOwnedGuestShutdownTests: XCTestCase {
    private func request(_ owner: HvfOwnedGuestShutdown) async -> Bool {
        await withCheckedContinuation { continuation in owner.request { continuation.resume(returning: $0) } }
    }

    func testExactFrozenFileReceivesOnlyOneCommand() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("control"), other = root.appendingPathComponent("other")
        try Data("existing\n".utf8).write(to: control)
        try Data("other\n".utf8).write(to: other)
        let owner = HvfOwnedGuestShutdown(path: control.path)
        defer { owner.close() }
        let delivered = await request(owner)
        XCTAssertTrue(delivered)
        let repeated = await request(owner)
        XCTAssertFalse(repeated)
        XCTAssertEqual(try String(contentsOf: control), "existing\nshutdown.exe /p /f\n")
        XCTAssertEqual(try String(contentsOf: other), "other\n")
    }

    func testReplacementSymlinkAndMissingIdentityNeverReceiveCommand() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("control"), held = root.appendingPathComponent("held")
        try Data("original\n".utf8).write(to: control)
        let owner = HvfOwnedGuestShutdown(path: control.path)
        defer { owner.close() }
        try FileManager.default.moveItem(at: control, to: held)
        try Data("replacement\n".utf8).write(to: control)
        let delivered = await request(owner)
        XCTAssertFalse(delivered)
        XCTAssertEqual(try String(contentsOf: held), "original\n")
        XCTAssertEqual(try String(contentsOf: control), "replacement\n")
        let missing = root.appendingPathComponent("missing")
        let absentOwner = HvfOwnedGuestShutdown(path: missing.path)
        defer { absentOwner.close() }
        let absent = await request(absentOwner)
        XCTAssertFalse(absent)
        XCTAssertFalse(FileManager.default.fileExists(atPath: missing.path))
        let link = root.appendingPathComponent("link")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: control)
        let linkedOwner = HvfOwnedGuestShutdown(path: link.path)
        defer { linkedOwner.close() }
        let linked = await request(linkedOwner)
        XCTAssertFalse(linked)
    }

    func testCloseLatchRejectsARequestEvenWhenWorkerHasNotDrained() async throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data().write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let owner = HvfOwnedGuestShutdown(path: file.path)
        owner.close()
        let delivered = await request(owner)
        XCTAssertFalse(delivered)
        XCTAssertTrue(try Data(contentsOf: file).isEmpty)
    }
}

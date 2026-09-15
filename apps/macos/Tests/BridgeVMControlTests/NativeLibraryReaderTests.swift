import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeLibraryReaderTests: XCTestCase {
    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("native-library-reader-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false,
                                               attributes: [.posixPermissions: 0o700])
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }

    private func writeConfig(root: URL, id: String, displayName: String,
                             savedID: String? = nil, installPending: Bool? = nil,
                             memoryMiB: Int? = 6144, cpuCount: Int? = 4) throws -> URL {
        let entry = root.appendingPathComponent(id)
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: false)
        let config = VMConfig(id: savedID, name: displayName, displayName: displayName,
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: entry.appendingPathComponent("absent-bundle.vmbridge").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: id, displayWidth: 1280, displayHeight: 800,
            installPending: installPending, memMiB: memoryMiB, cpuCount: cpuCount,
            networkEnabled: false, experimental3DAllowed: false)
        let file = entry.appendingPathComponent("vm.json")
        try JSONEncoder().encode(config).write(to: file)
        return file
    }

    func testMissingLibraryReturnsEmptyWithoutCreatingDirectories() throws {
        let parent = try directory()
        let missing = parent.appendingPathComponent("never-created/vms")
        let before = try FileManager.default.contentsOfDirectory(atPath: parent.path)
        let snapshot = try NativeLibraryReader.snapshot(rootURL: missing)
        XCTAssertTrue(snapshot.records.isEmpty)
        XCTAssertTrue(snapshot.issues.isEmpty)
        XCTAssertTrue(snapshot.complete)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: parent.path), before)
        XCTAssertThrowsError(try NativeLibraryReader.snapshot(rootURL: missing, id: "absent"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: missing.path))
    }

    func testNativeUnicodeDirectoryOverridesStalePersistedIdentityWithoutWrites() throws {
        let root = try directory()
        let file = try writeConfig(root: root, id: "개발-vm", displayName: "개발 Windows",
                                   savedID: "../../stale-name", installPending: true)
        let bytes = try Data(contentsOf: file)
        let before = try FileManager.default.contentsOfDirectory(atPath: file.deletingLastPathComponent().path)
        let snapshot = try NativeLibraryReader.snapshot(rootURL: root, id: "개발-vm")
        let record = try XCTUnwrap(snapshot.records.first)
        XCTAssertEqual(snapshot.records.count, 1)
        XCTAssertTrue(snapshot.complete)
        XCTAssertTrue(snapshot.issues.isEmpty)
        XCTAssertEqual(record.id, "개발-vm")
        XCTAssertEqual(record.displayName, "개발 Windows")
        XCTAssertEqual(record.backendKind, "hvf-engine")
        XCTAssertEqual(record.runtimeState, "unobserved")
        XCTAssertEqual(record.installPending, true)
        XCTAssertTrue(record.recoveryObservations.isEmpty)
        XCTAssertEqual(try Data(contentsOf: file), bytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: file.deletingLastPathComponent().path), before)
    }

    func testCorruptAndOversizeConfigurationsRemainIssuesAndUnchanged() throws {
        let root = try directory()
        let valid = try writeConfig(root: root, id: "valid", displayName: "Valid")
        let corrupt = try writeConfig(root: root, id: "corrupt", displayName: "Corrupt")
        let oversized = try writeConfig(root: root, id: "oversized", displayName: "Oversized")
        let corruptBytes = Data("{malformed".utf8)
        let oversizedBytes = Data(repeating: 0x20, count: VMLibrary.maximumConfigBytes + 1)
        try corruptBytes.write(to: corrupt)
        try oversizedBytes.write(to: oversized)
        let validBytes = try Data(contentsOf: valid)
        let before = try FileManager.default.contentsOfDirectory(atPath: root.path).sorted()
        let snapshot = try NativeLibraryReader.snapshot(rootURL: root)
        XCTAssertEqual(snapshot.records.map(\.id), ["valid"])
        XCTAssertEqual(snapshot.issues.count, 2)
        XCTAssertFalse(snapshot.complete)
        XCTAssertTrue(snapshot.issues.allSatisfy { !$0.code.isEmpty && !$0.path.isEmpty && !$0.message.isEmpty })
        XCTAssertEqual(try Data(contentsOf: corrupt), corruptBytes)
        XCTAssertTrue(try Data(contentsOf: oversized) == oversizedBytes, "Oversized input changed")
        XCTAssertEqual(try Data(contentsOf: valid), validBytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path).sorted(), before)
    }

    func testSymlinkEntriesAndFIFOConfigurationAreRefusedWithoutBlocking() throws {
        let root = try directory()
        let source = try writeConfig(root: root, id: "source", displayName: "Source")
        let sourceBytes = try Data(contentsOf: source)
        let linkedDirectory = root.appendingPathComponent("linked-directory")
        try FileManager.default.createSymbolicLink(at: linkedDirectory,
                                                   withDestinationURL: source.deletingLastPathComponent())
        let linkedEntry = root.appendingPathComponent("linked-config")
        try FileManager.default.createDirectory(at: linkedEntry, withIntermediateDirectories: false)
        let linkedConfig = linkedEntry.appendingPathComponent("vm.json")
        try FileManager.default.createSymbolicLink(at: linkedConfig, withDestinationURL: source)
        let fifoEntry = root.appendingPathComponent("fifo")
        try FileManager.default.createDirectory(at: fifoEntry, withIntermediateDirectories: false)
        let fifo = fifoEntry.appendingPathComponent("vm.json")
        XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
        let before = try FileManager.default.contentsOfDirectory(atPath: root.path).sorted()
        let began = ProcessInfo.processInfo.systemUptime
        let snapshot = try NativeLibraryReader.snapshot(rootURL: root)
        XCTAssertLessThan(ProcessInfo.processInfo.systemUptime - began, 2, "Special-file query did not return promptly")
        XCTAssertEqual(snapshot.records.map(\.id), ["source"])
        XCTAssertEqual(snapshot.issues.count, 3)
        XCTAssertFalse(snapshot.complete)
        XCTAssertEqual(try Data(contentsOf: source), sourceBytes)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: linkedConfig.path), source.path)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: linkedDirectory.path), source.deletingLastPathComponent().path)
        var status = stat()
        XCTAssertEqual(lstat(fifo.path, &status), 0)
        XCTAssertEqual(status.st_mode & S_IFMT, S_IFIFO)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path).sorted(), before)
    }

    func testNoncanonicalEntryAndMissingInspectIDDoNotChangeInventory() throws {
        let root = try directory()
        let file = try writeConfig(root: root, id: "Not Canonical", displayName: "Invalid directory")
        let bytes = try Data(contentsOf: file)
        let snapshot = try NativeLibraryReader.snapshot(rootURL: root)
        XCTAssertTrue(snapshot.records.isEmpty)
        XCTAssertEqual(snapshot.issues.count, 1)
        XCTAssertFalse(snapshot.complete)
        XCTAssertThrowsError(try NativeLibraryReader.snapshot(rootURL: root, id: "missing"))
        XCTAssertThrowsError(try NativeLibraryReader.snapshot(rootURL: root, id: "../escape"))
        XCTAssertEqual(try Data(contentsOf: file), bytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["Not Canonical"])
    }

    func testRelocationMarkerIsObservedWithoutReconciliationOrMutation() throws {
        let root = try directory()
        let file = try writeConfig(root: root, id: "relocating", displayName: "Relocating")
        let marker = file.deletingLastPathComponent().appendingPathComponent(".relocation-pending.json")
        let markerBytes = Data("retained pending relocation evidence".utf8)
        try markerBytes.write(to: marker)
        let configBytes = try Data(contentsOf: file)
        let before = try FileManager.default.contentsOfDirectory(atPath: file.deletingLastPathComponent().path).sorted()
        let snapshot = try NativeLibraryReader.snapshot(rootURL: root, id: "relocating")
        let record = try XCTUnwrap(snapshot.records.first)
        XCTAssertEqual(record.recoveryObservations, ["relocation-record-present-or-unreadable"])
        XCTAssertTrue(snapshot.complete)
        XCTAssertEqual(record.runtimeState, "unobserved")
        XCTAssertEqual(try Data(contentsOf: marker), markerBytes)
        XCTAssertEqual(try Data(contentsOf: file), configBytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: file.deletingLastPathComponent().path).sorted(), before)
    }

    func testSortedJSONPreservesRawOptionalStateWithoutRuntimeInference() throws {
        let root = try directory()
        _ = try writeConfig(root: root, id: "zulu", displayName: "Zulu", installPending: false,
                            memoryMiB: 3072, cpuCount: 2)
        _ = try writeConfig(root: root, id: "alpha", displayName: "Alpha", memoryMiB: nil, cpuCount: nil)
        let snapshot = try NativeLibraryReader.snapshot(rootURL: root)
        XCTAssertEqual(snapshot.records.map(\.id), ["alpha", "zulu"])
        XCTAssertNil(snapshot.records[0].installPending)
        XCTAssertEqual(snapshot.records[1].installPending, false)
        XCTAssertNil(snapshot.records[0].memoryMiB)
        XCTAssertNil(snapshot.records[0].cpuCount)
        XCTAssertEqual(snapshot.records[1].memoryMiB, 3072)
        XCTAssertEqual(snapshot.records[1].cpuCount, 2)
        XCTAssertTrue(snapshot.records.allSatisfy { $0.runtimeState == "unobserved" })
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let first = try encoder.encode(snapshot)
        let second = try encoder.encode(NativeLibraryReader.snapshot(rootURL: root))
        XCTAssertEqual(first, second)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: first) as? [String: Any])
        XCTAssertEqual(object["schema"] as? String, "bridgevm.app-library.v1")
        XCTAssertEqual(object["complete"] as? Bool, true)
        let records = try XCTUnwrap(object["records"] as? [[String: Any]])
        XCTAssertEqual(records.compactMap { $0["id"] as? String }, ["alpha", "zulu"])
        XCTAssertNil(object["raw_config"])
    }
}

import Darwin
import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionReferenceReadTests: XCTestCase {
    func testMalformedAndOversizedSiblingRecordsFailClosed() throws {
        for request in [false, true] {
            for bytes in [Data(), Data("{}".utf8), Data(repeating: 0, count: VMLibrary.maximumConfigBytes + 1)] {
                let f = try HvfPendingDeletionFixture()
                let sibling = try createSibling(in: f)
                let record = recordURL(sibling, request: request, in: f)
                try bytes.write(to: record)
                try f.assertRefused()
                XCTAssertTrue(try Data(contentsOf: record) == bytes, "Refusal must not repair records")
            }
        }
    }

    func testSiblingRecordSymlinksDirectoriesAndFIFOsFailClosedWithoutBlocking() throws {
        for request in [false, true] {
            for replacement in ["directory", "link", "fifo"] {
                let f = try HvfPendingDeletionFixture()
                let sibling = try createSibling(in: f)
                let record = recordURL(sibling, request: request, in: f)
                let copy = f.root.appendingPathComponent("original-record")
                try f.fm.moveItem(at: record, to: copy)
                switch replacement {
                case "directory": try f.fm.createDirectory(at: record, withIntermediateDirectories: false)
                case "link": try f.fm.createSymbolicLink(at: record, withDestinationURL: copy)
                default: XCTAssertEqual(mkfifo(record.path, 0o600), 0)
                }
                let began = ProcessInfo.processInfo.systemUptime
                try f.assertRefused()
                XCTAssertLessThan(ProcessInfo.processInfo.systemUptime - began, 2)
                XCTAssertTrue(f.fm.fileExists(atPath: copy.path))
            }
        }
    }

    func testMissingPendingRequestFailsClosedButInstalledSiblingNeedsNoRequest() throws {
        let f = try HvfPendingDeletionFixture()
        var sibling = try createSibling(in: f)
        try f.fm.removeItem(at: recordURL(sibling, request: true, in: f))
        try f.assertRefused()
        sibling.installPending = false
        try JSONEncoder().encode(sibling).write(to: recordURL(sibling, request: false, in: f))
        XCTAssertTrue(try f.delete())
        XCTAssertTrue(f.fm.fileExists(atPath: sibling.bundlePath))
    }

    func testUnreadableActualCandidateOrRequestFailsClosed() throws {
        guard geteuid() != 0 else { throw XCTSkip("Permission test requires an unprivileged user") }
        for target in ["entry", "config", "metadata", "request"] {
            let f = try HvfPendingDeletionFixture()
            let sibling = try createSibling(in: f)
            let record = recordURL(sibling, request: target == "metadata" || target == "request", in: f)
            let locked = target == "entry" || target == "metadata" ? record.deletingLastPathComponent() : record
            let original = try f.fm.attributesOfItem(atPath: locked.path)[.posixPermissions]
            try f.fm.setAttributes([.posixPermissions: 0o000], ofItemAtPath: locked.path)
            defer { if let original { try? f.fm.setAttributes([.posixPermissions: original], ofItemAtPath: locked.path) } }
            try f.assertRefused()
        }
    }

    func testAmbiguousRegistrationDirectoryFailsClosed() throws {
        for alias in [false, true] {
            let f = try HvfPendingDeletionFixture()
            let sibling = try createSibling(in: f)
            let entry = f.library.appendingPathComponent(sibling.slug)
            if alias {
                let copy = f.root.appendingPathComponent("saved-entry")
                try f.fm.moveItem(at: entry, to: copy)
                try f.fm.createSymbolicLink(at: entry, withDestinationURL: copy)
            } else {
                try f.fm.moveItem(at: entry, to: f.library.appendingPathComponent("Not Canonical"))
            }
            try f.assertRefused()
        }
    }

    func testSiblingJournalIsNotRecoveredWhileCheckingReferences() throws {
        let f = try HvfPendingDeletionFixture()
        let sibling = try createSibling(in: f)
        let journal = URL(fileURLWithPath: sibling.bundlePath)
            .appendingPathComponent("metadata/hvf-install-finalization/journal.json")
        try f.fm.createDirectory(at: journal.deletingLastPathComponent(), withIntermediateDirectories: true)
        let bytes = Data("unreadable recovery journal; not this check's input".utf8)
        try bytes.write(to: journal)
        let configURL = recordURL(sibling, request: false, in: f)
        let configBytes = try Data(contentsOf: configURL)
        XCTAssertTrue(try f.delete())
        XCTAssertEqual(try Data(contentsOf: journal), bytes)
        XCTAssertEqual(try Data(contentsOf: configURL), configBytes)
    }

    private func createSibling(in f: HvfPendingDeletionFixture) throws -> VMConfig {
        _ = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(name: "Sibling", isoPath: f.iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil, libraryRoot: f.library))
        return try NativeLibraryReader.readConfig(rootURL: f.library, id: "sibling")
    }

    private func recordURL(_ sibling: VMConfig, request: Bool, in f: HvfPendingDeletionFixture) -> URL {
        request ? URL(fileURLWithPath: sibling.bundlePath).appendingPathComponent(HvfWindowsInstallRequest.fileName)
            : f.library.appendingPathComponent(sibling.slug).appendingPathComponent("vm.json")
    }
}

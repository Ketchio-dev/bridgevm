import XCTest
@testable import BridgeVMControl

final class VMCloneDestinationTests: XCTestCase {
    private final class Fixture {
        let fm = FileManager.default
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        var source: URL { root.appendingPathComponent("source") }
        var library: URL { root.appendingPathComponent("library") }
        var sentinel: URL { source.appendingPathComponent("sentinel") }
        init() throws {
            try fm.createDirectory(at: source, withIntermediateDirectories: true)
            try Data("keep source unchanged".utf8).write(to: sentinel)
        }
        deinit { try? fm.removeItem(at: root) }
        func config(windows: Bool) -> VMConfig {
            VMConfig(id: "source", name: "Source", displayName: "Source",
                backendKind: windows ? "hvf-engine" : "fast-vz", bundlePath: source.path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
                sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600)
        }
        func alias(_ name: String, to destination: URL) throws -> URL {
            let link = root.appendingPathComponent(name)
            try fm.createSymbolicLink(at: link, withDestinationURL: destination)
            return link
        }
    }

    // The real clone entry points run through reservation. Always stop before cp:
    // a regressed guard must never recursively copy its own source in this test.
    private func checkDestination(allowed: Bool, make: (Fixture) throws -> URL) throws {
        for windows in [false, true] {
            let fixture = try Fixture()
            let destination = try make(fixture)
            let sourceEntries = try fixture.fm.contentsOfDirectory(atPath: fixture.source.path).sorted()
            let storageExisted = fixture.fm.fileExists(atPath: destination.path)
            var reachedCopy = false
            let beforeCopy = {
                reachedCopy = true
                XCTAssertTrue(fixture.fm.fileExists(atPath: destination.appendingPathComponent("copy").path))
                XCTAssertFalse(fixture.fm.fileExists(atPath: destination.appendingPathComponent("copy/bundle.vmbridge").path))
                return false
            }
            let config = fixture.config(windows: windows)
            let result = windows
                ? VMLibrary.cloneWindowsHVF(name: "Copy", template: config, storageDir: destination,
                                           libraryRoot: fixture.library, beforeCopy: beforeCopy)
                : VMLibrary.cloneUbuntu(name: "Copy", template: config, storageDir: destination,
                                       libraryRoot: fixture.library, beforeCopy: beforeCopy)
            let label = windows ? "Windows HVF" : "Ubuntu"
            XCTAssertNil(result, label)
            XCTAssertEqual(reachedCopy, allowed, "\(label): unsafe destination reached pre-copy after reservation")
            XCTAssertEqual(fixture.fm.fileExists(atPath: fixture.library.path), allowed,
                           "\(label): rejected destination must not create the library")
            XCTAssertEqual(fixture.fm.fileExists(atPath: destination.path), storageExisted || allowed,
                           "\(label): rejected destination must not create storage")
            XCTAssertFalse(fixture.fm.fileExists(atPath: destination.appendingPathComponent("copy").path))
            XCTAssertEqual(try fixture.fm.contentsOfDirectory(atPath: fixture.source.path).sorted(), sourceEntries, label)
            XCTAssertEqual(try Data(contentsOf: fixture.sentinel), Data("keep source unchanged".utf8), label)
        }
    }

    func testExistingSiblingStorageIsAllowed() throws {
        try checkDestination(allowed: true) { fixture in
            let storage = fixture.root.appendingPathComponent("storage")
            try fixture.fm.createDirectory(at: storage, withIntermediateDirectories: false)
            return storage
        }
    }

    func testNonexistentSiblingWithSharedNamePrefixIsAllowed() throws {
        try checkDestination(allowed: true) { $0.root.appendingPathComponent("source-copy/missing/deeper") }
    }

    func testSameSourceAndDirectDescendantAreRejectedBeforeReservation() throws {
        try checkDestination(allowed: false) { $0.source }
        try checkDestination(allowed: false) { $0.source.appendingPathComponent("missing/deeper") }
    }

    func testAliasToSourceWithMissingSuffixIsRejectedBeforeReservation() throws {
        try checkDestination(allowed: false) { fixture in
            try fixture.alias("alias", to: fixture.source).appendingPathComponent("missing/deeper")
        }
    }

    func testExistingAliasToSourceIsRejectedBeforeReservation() throws {
        try checkDestination(allowed: false) { try $0.alias("alias", to: $0.source) }
    }

    func testDanglingAndCyclicAliasesAreRejectedBeforeReservation() throws {
        try checkDestination(allowed: false) { fixture in
            try fixture.alias("dangling", to: fixture.root.appendingPathComponent("absent"))
                .appendingPathComponent("missing/deeper")
        }
        try checkDestination(allowed: false) { fixture in
            try fixture.alias("cyclic", to: fixture.root.appendingPathComponent("cyclic"))
                .appendingPathComponent("missing/deeper")
        }
    }

    func testRegularFileAncestorIsRejectedBeforeReservation() throws {
        try checkDestination(allowed: false) { $0.sentinel.appendingPathComponent("missing/deeper") }
    }

    func testSafeUbuntuCloneCopiesAndRegistersInIsolatedLibrary() throws {
        let fixture = try Fixture()
        let metadata = fixture.source.appendingPathComponent("metadata")
        try fixture.fm.createDirectory(at: metadata, withIntermediateDirectories: false)
        for file in ["apple-vz-launch.json", "handoff.json"] {
            XCTAssertTrue(JSONFile.writeDict(["vm_name": "Source", "bundle_path": fixture.source.path],
                                              to: metadata.appendingPathComponent(file).path))
        }
        let storage = fixture.root.appendingPathComponent("source-copy/missing/deeper")
        let clone = try XCTUnwrap(VMLibrary.cloneUbuntu(name: "Copy", template: fixture.config(windows: false),
                                                        storageDir: storage, libraryRoot: fixture.library))
        XCTAssertEqual(clone.bundlePath, storage.appendingPathComponent("copy/bundle.vmbridge").path)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: clone.bundlePath).appendingPathComponent("sentinel")),
                       Data("keep source unchanged".utf8))
        XCTAssertEqual(JSONFile.loadDict(clone.launchSpecPath)?["bundle_path"] as? String, clone.bundlePath)
        XCTAssertEqual(JSONFile.loadDict(clone.handoffPath)?["vm_name"] as? String, "Copy")
        XCTAssertTrue(fixture.fm.fileExists(atPath: fixture.library.appendingPathComponent("copy/vm.json").path))
        XCTAssertEqual(try Data(contentsOf: fixture.sentinel), Data("keep source unchanged".utf8))
    }

    func testGenericContainmentStillAcceptsRegularFileCandidates() throws {
        let fixture = try Fixture()
        let outside = fixture.root.appendingPathComponent("manifest.json")
        try Data("{}".utf8).write(to: outside)
        XCTAssertTrue(VMLibrary.isSameOrDescendant(fixture.sentinel, of: fixture.source))
        XCTAssertFalse(VMLibrary.isSameOrDescendant(outside, of: fixture.source))
    }
}

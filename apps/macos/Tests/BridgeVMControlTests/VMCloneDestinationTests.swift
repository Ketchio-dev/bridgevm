import XCTest
@testable import BridgeVMControl

final class VMCloneDestinationTests: XCTestCase {
    // The real clone entry points run through reservation. Always stop before cp:
    // a regressed guard must never recursively copy its own source in this test.
    private func checkDestination(allowed: Bool, make: (VMCloneDestinationFixture) throws -> URL) throws {
        do {
            let fixture = try VMCloneDestinationFixture()
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
            let result = VMLibrary.cloneWindowsHVF(name: "Copy", template: fixture.config(windows: true),
                storageDir: destination, libraryRoot: fixture.library, beforeCopy: beforeCopy)
            let label = "Windows HVF"
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

    func testGenericContainmentStillAcceptsRegularFileCandidates() throws {
        let fixture = try VMCloneDestinationFixture()
        let outside = fixture.root.appendingPathComponent("manifest.json")
        try Data("{}".utf8).write(to: outside)
        XCTAssertTrue(VMLibrary.isSameOrDescendant(fixture.sentinel, of: fixture.source))
        XCTAssertFalse(VMLibrary.isSameOrDescendant(outside, of: fixture.source))
    }
}

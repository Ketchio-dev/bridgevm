import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfCloneVarsShapeTests: XCTestCase {
    func testMissingVarsRetainsUIAdmissionButCopyRefuses() throws {
        let f = try HvfCloneVarsFixture(.regular)
        defer { f.clean() }
        try f.fm.removeItem(at: f.sourceVars)
        try checkRefused(f, admissionAllowed: true)
    }

    func testMissingMetadataRetainsUIAdmissionButCopyRefuses() throws {
        let f = try HvfCloneVarsFixture(.regular)
        defer { f.clean() }
        try f.fm.removeItem(at: f.sourceVars.deletingLastPathComponent())
        try checkRefused(f, admissionAllowed: true)
    }

    func testVarsDirectoryIsPreservedAndRefused() throws {
        let f = try HvfCloneVarsFixture(.regular)
        defer { f.clean() }
        try f.fm.removeItem(at: f.sourceVars)
        try f.fm.createDirectory(at: f.sourceVars, withIntermediateDirectories: false)
        let marker = f.sourceVars.appendingPathComponent("owned-marker")
        try f.bytes.write(to: marker)
        try checkRefused(f, admissionAllowed: false)
        XCTAssertEqual(try Data(contentsOf: marker), f.bytes)
    }

    func testMetadataFileIsPreservedAndRefused() throws {
        let f = try HvfCloneVarsFixture(.regular)
        defer { f.clean() }
        let metadata = f.sourceVars.deletingLastPathComponent()
        try f.fm.removeItem(at: metadata)
        try f.bytes.write(to: metadata)
        try checkRefused(f, admissionAllowed: false)
        XCTAssertEqual(try Data(contentsOf: metadata), f.bytes)
    }

    func testDanglingVarsLinkIsPreservedAndRefused() throws {
        let f = try HvfCloneVarsFixture(.varsLink)
        defer { f.clean() }
        try f.fm.removeItem(at: f.target)
        let destination = try f.fm.destinationOfSymbolicLink(atPath: f.sourceVars.path)
        try checkRefused(f, admissionAllowed: false)
        XCTAssertEqual(try f.fm.destinationOfSymbolicLink(atPath: f.sourceVars.path), destination)
        XCTAssertFalse(f.fm.fileExists(atPath: f.target.path))
    }

    private func checkRefused(_ f: HvfCloneVarsFixture, admissionAllowed: Bool) throws {
        let entries = try f.fm.contentsOfDirectory(atPath: f.libraryRoot.path).sorted()
        XCTAssertEqual(HvfWindowsCloneSource.refusal(for: f.config, phase: .admission) == nil, admissionAllowed)
        XCTAssertNotNil(HvfWindowsCloneSource.refusal(for: f.config, phase: .copy))
        let result = VMLibrary.cloneWindowsHVF(name: "Copy", template: f.config, libraryRoot: f.libraryRoot,
            beforeCopy: { XCTFail("Unsafe media must refuse before reservation or copying"); return false },
            afterCopy: { XCTFail("Unsafe media must not reach key custody"); return false })
        XCTAssertNil(result)
        XCTAssertEqual(try f.fm.contentsOfDirectory(atPath: f.libraryRoot.path).sorted(), entries)
    }
}

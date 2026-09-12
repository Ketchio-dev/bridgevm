import XCTest
@testable import BridgeVMControl

final class HvfCloneValidationTests: XCTestCase {
    func testRejectedCopiedMediaIsRemovedBeforeRegistrationOrIdentityWork() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let source = root.appendingPathComponent("source.bundle")
        let library = root.appendingPathComponent("library")
        let fm = FileManager.default
        try fm.createDirectory(at: source.appendingPathComponent("disks"), withIntermediateDirectories: true)
        try fm.createDirectory(at: source.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        try fm.createDirectory(at: library, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        let disk = source.appendingPathComponent("disks/hvf-target.raw")
        let vars = source.appendingPathComponent("metadata/hvf-vars.fd")
        try Data("source-disk".utf8).write(to: disk)
        try Data("source-vars".utf8).write(to: vars)
        let config = VMConfig(id: "source", name: "Source", displayName: "Source",
            backendKind: "hvf-engine", bootMode: nil, bundlePath: source.path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600)
        var called = false
        let result = VMLibrary.cloneWindowsHVF(name: "Copy", template: config, libraryRoot: library) {
            called = true
            let copied = library.appendingPathComponent("copy/bundle.vmbridge")
            XCTAssertEqual(try? Data(contentsOf: copied.appendingPathComponent("disks/hvf-target.raw")), Data("source-disk".utf8))
            XCTAssertEqual(try? Data(contentsOf: copied.appendingPathComponent("metadata/hvf-vars.fd")), Data("source-vars".utf8))
            XCTAssertTrue(VMLibrary.list(rootURL: library).isEmpty)
            XCTAssertFalse(fm.fileExists(atPath: copied.appendingPathComponent("metadata/vtpm-lifecycle").path))
            return false
        }
        XCTAssertTrue(called)
        XCTAssertNil(result)
        XCTAssertTrue(VMLibrary.list(rootURL: library).isEmpty)
        XCTAssertTrue(try fm.contentsOfDirectory(atPath: library.path).isEmpty)
        XCTAssertEqual(try Data(contentsOf: disk), Data("source-disk".utf8))
        XCTAssertEqual(try Data(contentsOf: vars), Data("source-vars".utf8))
    }
}

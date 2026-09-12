import XCTest
@testable import BridgeVMControl

final class HvfCloneLeaseFailureTests: XCTestCase {
    private final class Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString).resolvingSymlinksInPath()
        var library: URL { root.appendingPathComponent("library") }
        var source: URL { root.appendingPathComponent("source.bundle") }
        var copied: URL { library.appendingPathComponent("copy/bundle.vmbridge") }
        var config: VMConfig {
            VMConfig(id: "source", name: "Source", displayName: "Source",
                backendKind: "hvf-engine", bootMode: nil, bundlePath: source.path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
                sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600)
        }
        init() throws {
            let fm = FileManager.default
            do {
                try fm.createDirectory(at: source.appendingPathComponent("disks"), withIntermediateDirectories: true)
                try fm.createDirectory(at: source.appendingPathComponent("metadata"), withIntermediateDirectories: true)
                try fm.createDirectory(at: library, withIntermediateDirectories: true)
                try Data("source-disk".utf8).write(to: source.appendingPathComponent("disks/hvf-target.raw"))
                try Data("source-vars".utf8).write(to: source.appendingPathComponent("metadata/hvf-vars.fd"))
            } catch { try? fm.removeItem(at: root); throw error }
        }
        deinit { try? FileManager.default.removeItem(at: root) }
        func session(exitStatus: Int) throws -> HvfMediaLeaseSession {
            let executable = root.appendingPathComponent("helper")
            let script = "#!/bin/sh\nprintf 'bridgevm-media-lease-v1\\n'\nread command\nexit \(exitStatus)\n"
            try Data(script.utf8).write(to: executable)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
            return try HvfMediaLeaseSession(executable: executable,
                disk: source.appendingPathComponent("disks/hvf-target.raw").path,
                vars: source.appendingPathComponent("metadata/hvf-vars.fd").path)
        }
        func assertOnlySourceRemains() throws {
            XCTAssertTrue(VMLibrary.list(rootURL: library).isEmpty)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: library.path).isEmpty)
            XCTAssertEqual(try Data(contentsOf: source.appendingPathComponent("disks/hvf-target.raw")), Data("source-disk".utf8))
            XCTAssertEqual(try Data(contentsOf: source.appendingPathComponent("metadata/hvf-vars.fd")), Data("source-vars".utf8))
        }
    }

    func testReleaseFailureRemovesCopiedCloneBeforePublication() throws {
        try assertFailedSessionCleansCopy(abortBeforeValidation: false)
    }

    func testAbortedOwnerRemovesCopiedCloneBeforePublication() throws {
        try assertFailedSessionCleansCopy(abortBeforeValidation: true)
    }

    private func assertFailedSessionCleansCopy(abortBeforeValidation: Bool) throws {
        let fixture = try Fixture()
        let session = try fixture.session(exitStatus: 7)
        var validated = false
        let result: VMConfig? = HvfMediaLeaseSession.copyWhileOwned(session: session) { validate in
            VMLibrary.cloneWindowsHVF(name: "Copy", template: fixture.config, libraryRoot: fixture.library) {
                validated = true
                XCTAssertEqual(try? Data(contentsOf: fixture.copied.appendingPathComponent("disks/hvf-target.raw")), Data("source-disk".utf8))
                XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty)
                if abortBeforeValidation { session.abort() }
                return validate()
            }
        }
        XCTAssertTrue(validated)
        XCTAssertNil(result)
        try fixture.assertOnlySourceRemains()
        XCTAssertThrowsError(try session.finish())
    }

    func testCoreRefusalDoesNotSkipOwnershipCleanup() throws {
        let fixture = try Fixture()
        let session = try fixture.session(exitStatus: 0)
        var invalid = fixture.config
        invalid.installPending = true
        var validated = false
        let result: VMConfig? = HvfMediaLeaseSession.copyWhileOwned(session: session) { validate in
            VMLibrary.cloneWindowsHVF(name: "Copy", template: invalid, libraryRoot: fixture.library) {
                validated = true
                return validate()
            }
        }
        XCTAssertFalse(validated)
        XCTAssertNil(result)
        try fixture.assertOnlySourceRemains()
        XCTAssertThrowsError(try session.finish())
    }
}

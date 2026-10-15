import CryptoKit
import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// Tiny owned files only. The copied helper is the harness-supplied real lease executable.
final class LibraryMoveSelectionAssets {
    let root: URL
    private var ownsRoot = false
    let diskBytes = Data("owned relocation disk".utf8)
    let varsBytes = Data("owned relocation vars".utf8)
    private(set) var helperDigest = ""
    var libraryRoot: URL { root.appendingPathComponent("library") }
    var destinationParent: URL { root.appendingPathComponent("moved") }
    var destination: URL { destinationParent.appendingPathComponent("bundle.vmbridge") }
    var repository: URL { root.appendingPathComponent("helper-repository") }
    var helper: URL { repository.appendingPathComponent("target/release/examples/snapshot_pair_cli") }
    var wrapperMarker: URL { root.appendingPathComponent("wrapper-was-executed") }

    init() throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        root = URL(fileURLWithPath: temporary).resolvingSymlinksInPath()
            .appendingPathComponent("move selection ' " + UUID().uuidString)
        guard mkdir(root.path, 0o700) == 0 else { throw CocoaError(.fileWriteFileExists) }
        ownsRoot = true
        // Do not silently choose a stale helper or build a new one from the test.
        _ = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_SNAPSHOT_HELPER"])
        let originalHelper = HvfMediaImportTestSupport.helper
        let wrapper = repository.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        for file in [helper, wrapper] {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        }
        helperDigest = try digest(originalHelper)
        try FileManager.default.copyItem(at: originalHelper, to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        try Data(("#!/bin/sh\nprintf 'unexpected wrapper\\n' > " + Shell.shQuote(wrapperMarker.path)
            + "\nexit 99\n").utf8).write(to: wrapper, options: .withoutOverwriting)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: wrapper.path)
        XCTAssertEqual(try digest(helper), helperDigest)
    }

    deinit { if ownsRoot { try? FileManager.default.removeItem(at: root) } }

    func configuration(_ id: String, kind: String) throws -> VMConfig {
        let bundle = libraryRoot.appendingPathComponent(id + "/bundle.vmbridge")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        for (file, bytes) in [(disk, diskBytes), (vars, varsBytes)] {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try bytes.write(to: file, options: .withoutOverwriting)
        }
        let config = VMConfig(id: id, name: id, displayName: id, backendKind: kind,
            bootMode: kind == "hvf-engine" ? "windows-hvf" : "direct-kernel", bundlePath: bundle.path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: id, displayWidth: 800, displayHeight: 600,
            installPending: false, diskPath: disk.path)
        guard VMLibrary.save(config, rootURL: libraryRoot) else { throw CocoaError(.fileWriteUnknown) }
        return config
    }

    @MainActor
    func execute(_ job: @escaping LibraryActionJob) async throws {
        let previous = ProcessInfo.processInfo.environment["BRIDGEVM_REPO_ROOT"]
        guard setenv("BRIDGEVM_REPO_ROOT", repository.path, 1) == 0 else { throw CocoaError(.fileWriteUnknown) }
        defer {
            if let previous { setenv("BRIDGEVM_REPO_ROOT", previous, 1) }
            else { unsetenv("BRIDGEVM_REPO_ROOT") }
        }
        guard HvfEngineSession.defaultRepoRoot() == repository else { throw CocoaError(.fileReadNoPermission) }
        // Actual worker path: protected transfer acquires/releases the real helper lease.
        await Task.detached(operation: job).value
    }

    func assertReadable(_ config: VMConfig, file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(config.diskPath))), diskBytes, file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: config.bundlePath)
            .appendingPathComponent("metadata/hvf-vars.fd")), varsBytes, file: file, line: line)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: libraryRoot, id: config.slug), config, file: file, line: line)
        XCTAssertFalse(FileManager.default.fileExists(atPath: URL(fileURLWithPath: config.bundlePath)
            .appendingPathComponent("metadata/vtpm").path), file: file, line: line)
    }

    func assertHelperPreserved(file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(try digest(helper), helperDigest, file: file, line: line)
        XCTAssertEqual(try digest(HvfMediaImportTestSupport.helper), helperDigest, file: file, line: line)
        XCTAssertFalse(FileManager.default.fileExists(atPath: wrapperMarker.path), file: file, line: line)
    }

    private func digest(_ url: URL) throws -> String {
        SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined()
    }
}

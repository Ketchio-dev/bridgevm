import XCTest
@testable import BridgeVMProductE2E

final class A9ImportedKeyCleanupTests: XCTestCase {
    func testCleanupRunsOnlyForPublishedImport() throws {
        let fixture = try A9CleanupFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.capture.path))
        try FileManager.default.createDirectory(at: fixture.config.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: fixture.config)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        let arguments = try String(contentsOf: fixture.capture, encoding: .utf8)
        XCTAssertTrue(arguments.contains("--vtpm-lifecycle forget-import"))
        XCTAssertTrue(arguments.contains("--stable-vm-id \(fixture.request.vmSlug)"))
    }
}

private struct A9CleanupFixture {
    let root: URL, capture: URL, config: URL, request: A9ImportRequest
    init() throws {
        root = URL(fileURLWithPath: "/tmp/bridgevm-import-e2e-cleanup-\(UUID().uuidString)")
        capture = root.appendingPathComponent("arguments.txt")
        let executable = root.appendingPathComponent("cleanup.sh")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("#!/bin/sh\nprintf '%s ' \"$@\" > \"\(capture.path)\"\n".utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        let nonce = String(repeating: "a", count: 64), slug = "bridgevm-a9-import-lane-1-aaaaaaaaaaaa"
        let library = root.appendingPathComponent("library"), bundle = library.appendingPathComponent(slug).appendingPathComponent("bundle")
        config = library.appendingPathComponent(slug).appendingPathComponent("vm.json")
        request = A9ImportRequest(schemaVersion: "bridgevm.windows-hvf-import-product-e2e-request.v1",
            jobID: "cleanup", commit: String(repeating: "b", count: 40), campaignMode: "pilot", lane: 1,
            nonce: nonce, vmName: "BridgeVM A9 Import Lane 1 aaaaaaaaaaaa", vmSlug: slug,
            threeDInjection: false, appBundlePath: root.path, appExecutablePath: executable.path,
            runnerPath: executable.path, sourceDiskPath: "disk", sourceVarsPath: "vars",
            sourceVtpmPath: "vtpm", sourceVtpmPackagePath: "package", sourceVtpmCodePath: "code",
            laneRoot: root.path, libraryRootPath: library.path, sharePath: root.appendingPathComponent("share").path,
            diskPath: bundle.appendingPathComponent("disks/hvf-target.raw").path,
            varsPath: bundle.appendingPathComponent("metadata/hvf-vars.fd").path,
            vtpmStatePath: bundle.appendingPathComponent("metadata/vtpm").path,
            snapshotPath: bundle.appendingPathComponent("metadata/snapshots/latest.snapshot").path,
            guestEvidencePath: bundle.appendingPathComponent("metadata/evidence.json").path)
    }
}

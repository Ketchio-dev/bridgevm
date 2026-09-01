import XCTest
@testable import BridgeVMControl

final class HvfWindowsGuestPayloadIdentityTests: XCTestCase {
    func testIdentityTracksManifestAndEveryPayloadFile() throws {
        let root = try makeFixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let payload = root.appendingPathComponent("payload", isDirectory: true)
        let manifest = root.appendingPathComponent("manifest.tsv")
        let first = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: payload.path, manifestPath: manifest.path)
        XCTAssertNil(first.error)

        try Data("changed".utf8).write(to: payload.appendingPathComponent("storage/driver.sys"))
        let changedFile = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: payload.path, manifestPath: manifest.path)
        XCTAssertNil(changedFile.error)
        XCTAssertNotEqual(changedFile.digest, first.digest)

        try Data("changed manifest".utf8).write(to: manifest)
        let changedManifest = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: payload.path, manifestPath: manifest.path)
        XCTAssertNotEqual(changedManifest.digest, changedFile.digest)
    }

    func testIdentityRejectsMissingInputAndSymlink() throws {
        XCTAssertNotNil(HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: nil, manifestPath: nil).error)
        let root = try makeFixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let payload = root.appendingPathComponent("payload", isDirectory: true)
        try FileManager.default.createSymbolicLink(
            at: payload.appendingPathComponent("alias.sys"),
            withDestinationURL: payload.appendingPathComponent("storage/driver.sys"))
        XCTAssertNotNil(HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: payload.path,
            manifestPath: root.appendingPathComponent("manifest.tsv").path).error)
    }

    func testProductStagingOwnsAnImmutableCopy() throws {
        let root = try makeFixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let bundle = root.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(
            at: bundle.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        let source = root.appendingPathComponent("payload", isDirectory: true)
        let manifest = root.appendingPathComponent("manifest.tsv")
        let staged = try XCTUnwrap(WindowsHVFGuestPayloadPolicy.stage(
            payloadDirectory: source.path, manifestPath: manifest.path, in: bundle.path))
        try FileManager.default.removeItem(at: source)
        try FileManager.default.removeItem(at: manifest)
        XCTAssertNil(HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: staged.directory, manifestPath: staged.manifest).error)
        XCTAssertEqual(HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: staged.directory, manifestPath: staged.manifest).digest, staged.identity)
    }

    func testInstallFactoryPersistsOnlyManagedPayloadPathsAndIdentity() throws {
        let root = try makeFixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let iso = root.appendingPathComponent("windows.iso")
        try Data("iso".utf8).write(to: iso)
        let library = root.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        let config = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "Payload Managed", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            guestPayloadDirectory: root.appendingPathComponent("payload").path,
            guestPayloadManifest: root.appendingPathComponent("manifest.tsv").path,
            storageDir: library, libraryRoot: library, persist: false))
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(request.guestPayloadDirectory,
                       config.bundlePath + "/metadata/windows-guest-payload")
        XCTAssertEqual(request.guestPayloadManifest,
                       config.bundlePath + "/metadata/windows-guest-payload.tsv")
        XCTAssertEqual(request.guestPayloadIdentity,
                       HvfWindowsGuestPayloadIdentity.inspect(
                        payloadDirectory: request.guestPayloadDirectory,
                        manifestPath: request.guestPayloadManifest).digest)
    }

    func testSourceCacheKeyAndBuildEnvironmentBindGuestPayload() throws {
        let root = try makeFixture()
        defer { try? FileManager.default.removeItem(at: root) }
        XCTAssertNotEqual(
            HvfWindowsInstallCacheIdentity.key(
                isoSHA256: "iso", guestPayloadIdentity: "payload-a", repoRoot: root),
            HvfWindowsInstallCacheIdentity.key(
                isoSHA256: "iso", guestPayloadIdentity: "payload-b", repoRoot: root))
        let bundle = root.appendingPathComponent("bundle.vmbridge")
        let payload = bundle.appendingPathComponent("metadata/windows-guest-payload").path
        let manifest = bundle.appendingPathComponent("metadata/windows-guest-payload.tsv").path
        let request = HvfWindowsInstallRequest(
            isoPath: root.appendingPathComponent("windows.iso").path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
            guestPayloadDirectory: payload, guestPayloadManifest: manifest,
            guestPayloadIdentity: "payload-a")
        let plan = HvfWindowsInstallPlan(
            repoRoot: root, libraryRoot: root.appendingPathComponent("library"),
            bundlePath: bundle.path, slug: "payload", request: request)
        let environment = plan.sourceBuildCommand().environment
        XCTAssertEqual(environment["WINDOWS_GUEST_PAYLOAD_DIR"], payload)
        XCTAssertEqual(environment["WINDOWS_GUEST_PAYLOAD_MANIFEST"], manifest)
        XCTAssertTrue(plan.sourceImagePath.contains("/Derived/WindowsInstallSources/win11-"))
    }

    private func makeFixture() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-guest-payload-\(UUID())", isDirectory: true)
        let storage = root.appendingPathComponent("payload/storage", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        try Data("driver".utf8).write(to: storage.appendingPathComponent("driver.sys"))
        try Data("manifest".utf8).write(to: root.appendingPathComponent("manifest.tsv"))
        return root
    }
}

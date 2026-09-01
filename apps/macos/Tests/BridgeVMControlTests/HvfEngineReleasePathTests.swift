import XCTest
@testable import BridgeVMControl

final class HvfEngineReleasePathTests: XCTestCase {
    func testReleaseIgnoresCheckoutAndEnvironmentRoots() throws {
        let temporary = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temporary) }
        let checkout = temporary.appendingPathComponent("checkout", isDirectory: true)
        let resources = temporary.appendingPathComponent("BridgeVM.app/Contents/Resources", isDirectory: true)
        try FileManager.default.createDirectory(
            at: checkout.appendingPathComponent("scripts"), withIntermediateDirectories: true)
        try Data("#!/bin/sh\n".utf8).write(
            to: checkout.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh"))
        try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)

        let resolved = HvfEngineSession.defaultRepoRoot(
            currentDirectoryPath: checkout.path,
            environment: ["BRIDGEVM_REPO_ROOT": checkout.path],
            executablePath: checkout.appendingPathComponent("BridgeVMControl").path,
            resourcePath: resources.path,
            allowDevelopmentOverrides: false)
        XCTAssertEqual(resolved, resources.standardizedFileURL)
        XCTAssertFalse(resolved.path.hasPrefix(checkout.path))
    }

    func testReleaseWithoutBundleResourcesFailsClosed() {
        let resolved = HvfEngineSession.defaultRepoRoot(
            currentDirectoryPath: "/tmp/attacker-checkout",
            environment: ["BRIDGEVM_REPO_ROOT": "/tmp/attacker-checkout"],
            executablePath: "/tmp/attacker-checkout/BridgeVMControl",
            resourcePath: nil,
            allowDevelopmentOverrides: false)
        XCTAssertEqual(resolved.path, "/BridgeVMUnavailableResources")
    }
}

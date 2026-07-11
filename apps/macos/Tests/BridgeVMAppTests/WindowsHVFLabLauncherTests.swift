import XCTest
@testable import BridgeVMApp

final class WindowsHVFLabLauncherTests: XCTestCase {
    func testResolvesExecutableInsideNestedApplication() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let host = root.appendingPathComponent("BridgeVM.app", isDirectory: true)
        let executable = host
            .appendingPathComponent(WindowsHVFLabInstallation.relativeBundlePath, isDirectory: true)
            .appendingPathComponent("Contents/MacOS/BridgeVMControl")
        try FileManager.default.createDirectory(
            at: executable.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)

        let installation = try WindowsHVFLabInstallation.resolve(hostBundleURL: host)

        XCTAssertEqual(installation.bundleURL.path, host.path + "/Contents/Applications/BridgeVMControl.app")
        XCTAssertEqual(installation.executableURL.path, executable.path)
    }

    func testRejectsMissingNestedApplication() {
        let host = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
            .appendingPathComponent("BridgeVM.app", isDirectory: true)

        XCTAssertThrowsError(try WindowsHVFLabInstallation.resolve(hostBundleURL: host)) { error in
            guard case let WindowsHVFLabLaunchError.missingBundle(path) = error else {
                return XCTFail("unexpected error: \(error)")
            }
            XCTAssertTrue(path.hasSuffix("Contents/Applications/BridgeVMControl.app"))
        }
    }

    func testRejectsNonExecutableNestedApplication() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let host = root.appendingPathComponent("BridgeVM.app", isDirectory: true)
        let executable = host
            .appendingPathComponent(WindowsHVFLabInstallation.relativeBundlePath, isDirectory: true)
            .appendingPathComponent("Contents/MacOS/BridgeVMControl")
        try FileManager.default.createDirectory(
            at: executable.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data().write(to: executable)

        XCTAssertThrowsError(try WindowsHVFLabInstallation.resolve(hostBundleURL: host)) { error in
            XCTAssertEqual(error as? WindowsHVFLabLaunchError, .missingExecutable(executable.path))
        }
    }
}

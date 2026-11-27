import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPackagedWrapperPolicyTests: XCTestCase {
    func testWrapperChildEnvironmentDropsExecutableOverrides() {
        let env = HvfPackagedWrapperPolicy.environment([
            "BRIDGEVM_PREBUILT_PROBE": "/tmp/probe", "CARGO_TARGET_DIR": "/tmp/target",
            "PATH": "/usr/bin:/bin", "LANG": "en_US.UTF-8"])
        XCTAssertEqual(env, ["PATH": "/usr/bin:/bin", "LANG": "en_US.UTF-8"])
    }

    func testOnlyCurrentSignedAppCanAuthorizePackagedResources() throws {
        let store = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: store) }
        let app = store.appendingPathComponent("BridgeVM.app")
        let contents = app.appendingPathComponent("Contents")
        let resources = contents.appendingPathComponent("Resources")
        let executable = contents.appendingPathComponent("MacOS/BridgeVMControl")
        try FileManager.default.createDirectory(at: executable.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: URL(fileURLWithPath: "/usr/bin/true"), to: executable)
        let info = "<plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>dev.bridgevm.policy-test</string><key>CFBundleExecutable</key><string>BridgeVMControl</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"
        try Data(info.utf8).write(to: contents.appendingPathComponent("Info.plist"))
        let notice = resources.appendingPathComponent("notice.txt")
        try Data("signed".utf8).write(to: notice)
        let sign = Process()
        sign.executableURL = URL(fileURLWithPath: "/usr/bin/codesign")
        sign.arguments = ["--force", "--deep", "--sign", "-", app.path]
        sign.standardOutput = FileHandle.nullDevice
        sign.standardError = FileHandle.nullDevice
        try sign.run()
        sign.waitUntilExit()
        XCTAssertEqual(sign.terminationStatus, 0)
        XCTAssertTrue(HvfPackagedWrapperPolicy.codeSignatureVerified(app: app))
        XCTAssertFalse(HvfPackagedWrapperPolicy.signatureVerified(repoRoot: resources))
        try Data("tampered".utf8).write(to: notice)
        XCTAssertFalse(HvfPackagedWrapperPolicy.codeSignatureVerified(app: app))
    }
}

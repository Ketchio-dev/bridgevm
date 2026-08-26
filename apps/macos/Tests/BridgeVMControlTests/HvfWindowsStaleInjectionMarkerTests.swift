import XCTest
@testable import BridgeVMControl
final class HvfWindowsStaleInjectionMarkerTests: XCTestCase {
    func testExistingMarkerAndInjectorCannotAuthorizeProductInjection() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let metadata = root.appendingPathComponent("metadata", isDirectory: true)
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let injector = root.appendingPathComponent("injector.raw")
        try Data(count: 512).write(to: injector)
        try Data("\(injector.path)\n".utf8).write(
            to: root.appendingPathComponent("metadata/hvf-inject-pending"))
        let config = try XCTUnwrap(HvfEngineConfig.libraryVM(VMConfig(
            id: "vm", name: "vm", displayName: "vm", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.path, runnerPath: "",
            launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "vm", displayWidth: 1280, displayHeight: 800,
            installPending: false)))
        XCTAssertFalse(config.wrapperArguments().contains("--placeholder-nsid1"))
        XCTAssertFalse(config.wrapperArguments().contains("--boot-timer-desktop-agent"))
    }
}

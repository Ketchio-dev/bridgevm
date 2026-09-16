import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLICreateWindowsFailureTests: XCTestCase {
    func testCreatorFailurePublishesNothing() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let request = makeRequest()
        XCTAssertThrowsError(try NativeCLICreateWindows.create(request, libraryRoot: root,
            creator: { _, _ in nil }))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }

    func testReadbackMismatchCannotBeReportedAsSuccess() throws {
        let request = makeRequest()
        let config = VMConfig(id: "vm", name: "VM", displayName: "VM", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/tmp/vm.bundle", runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "vm",
            displayWidth: 1440, displayHeight: 900, installPending: true, diskPath: "/tmp/vm.raw",
            memMiB: 6_144, cpuCount: 4, networkEnabled: true)
        var replaced = config
        replaced.memMiB = 8_192
        XCTAssertThrowsError(try NativeCLICreateWindows.create(request,
            libraryRoot: URL(fileURLWithPath: "/tmp/library"),
            creator: { _, _ in config }, reader: { _, _ in replaced }))
    }

    private func makeRequest() -> NativeCLICreateWindowsOptions {
        .init(name: "VM", isoPath: "/tmp/Windows.iso", diskGiB: 64, memoryMiB: 6_144,
              cpuCount: 4, resolution: .init(width: 1440, height: 900), networkEnabled: true)
    }
}

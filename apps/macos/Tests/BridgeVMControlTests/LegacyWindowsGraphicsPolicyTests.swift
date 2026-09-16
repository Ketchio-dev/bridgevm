import XCTest
@testable import BridgeVMControl

final class LegacyWindowsGraphicsPolicyTests: XCTestCase {
    func testMissingGraphicsPolicyDefaultsThreeDOff() throws {
        let config = VMConfig(
            id: "legacy", name: "Legacy", displayName: "Legacy", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/tmp/legacy.bridgevm",
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "legacy",
            displayWidth: 1280, displayHeight: 800, installPending: false,
            diskPath: "/tmp/legacy.raw", memMiB: 4096, cpuCount: 4,
            networkEnabled: true, experimental3DAllowed: nil)

        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(config))
        XCTAssertFalse(launch.virtioGpu3d)
        XCTAssertFalse(launch.allowsExperimental3D)
        for forbidden in ["--virtio-gpu-3d", "--virtio-gpu-device-id", "1050", "virgl"] {
            XCTAssertFalse(launch.wrapperArguments().contains(forbidden))
        }
    }
}

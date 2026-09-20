import XCTest
@testable import BridgeVMControl

final class GraphicsLabOptInPolicyTests: XCTestCase {
    func testGraphicsLabRequiresAnExplicitThreeDOptIn() {
        let config = HvfEngineView.defaultConfig()
        XCTAssertFalse(config.virtioGpu3d)
        XCTAssertTrue(config.allowsExperimental3D)
        XCTAssertFalse(config.wrapperArguments().contains("--virtio-gpu-3d"))
    }

    func testExplicitLegacyGraphicsLabOptInIsPreserved() throws {
        let config = VMConfig(
            id: "lab", name: "Lab", displayName: "Lab", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/tmp/lab.bridgevm",
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "lab",
            displayWidth: 1280, displayHeight: 800, installPending: false,
            diskPath: "/tmp/lab.raw", memMiB: 4096, cpuCount: 4,
            networkEnabled: true, experimental3DAllowed: true)

        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(config))
        XCTAssertTrue(launch.virtioGpu3d)
        XCTAssertTrue(launch.allowsExperimental3D)
        XCTAssertTrue(launch.wrapperArguments().contains("--virtio-gpu-3d"))
    }
}

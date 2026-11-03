import XCTest
@testable import BridgeVMControl

/// The per-VM display-driver flag attaches the 2D virtio-gpu through both launch paths.
final class HvfVirtioGpu2dTests: XCTestCase {
    private func vm(driver: Bool?, threeD: Bool?) -> VMConfig {
        var config = VMConfig(
            id: "win", name: "Windows", displayName: "Windows", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/tmp/win.bridgevm",
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "win",
            displayWidth: 1280, displayHeight: 800, installPending: false,
            diskPath: "/tmp/win.raw", memMiB: 4096, cpuCount: 4,
            networkEnabled: true, experimental3DAllowed: threeD)
        config.virtioDisplayDriverInstalled = driver
        return config
    }

    /// Wrapper, legacy runner and owned runner argument vectors.
    private func vectors(_ launch: HvfEngineConfig) -> [[String]] {
        [false, true].map {
            launch.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f",
                                   probePath: "p", ownedRuntime: $0)
        } + [launch.wrapperArguments()]
    }

    func testOnlyAnInstalledDriverWithThreeDOffAttachesThe2DDevice() throws {
        let cases: [(driver: Bool?, threeD: Bool?, attached: Bool)] = [
            (nil, nil, false), (false, nil, false), (true, nil, true), (true, false, true), (true, true, false)
        ]
        for c in cases {
            let label = "driver=\(String(describing: c.driver)) 3D=\(String(describing: c.threeD))"
            let library = try XCTUnwrap(HvfEngineConfig.libraryVM(vm(driver: c.driver, threeD: c.threeD)))
            var pending = vm(driver: c.driver, threeD: c.threeD); pending.installPending = true
            let fallback = HvfWindowsBackend(pending, processIsRunning: { _ in false }).makeHvfEngineConfig()
            for launch in [library, fallback] {
                XCTAssertEqual(launch.virtioGpu2d, c.attached, label)
                for args in vectors(launch) {
                    XCTAssertEqual(args.contains("--virtio-gpu"), c.attached, "\(label): \(args)")
                }
            }
        }
    }

    func testThreeDWinsWhenBothAreSet() {
        let launch = HvfEngineConfig(
            targetDiskPath: "t", uefiVarsPath: "v", evidenceDir: "e", watchdogMs: nil,
            ramMiB: 4096, smpCpus: 4, clipboardSync: false, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: false, virtioGpu3d: true, virtioGpu2d: true, nvmeBufferedIO: false,
            ctlFilePath: "c")
        for args in vectors(launch) {
            XCTAssertTrue(args.contains("--virtio-gpu-3d"), "\(args)")
            XCTAssertFalse(args.contains("--virtio-gpu"), "\(args)")
        }
    }

    func testVMConfigRoundTripsTheFlagAndOldFilesReadAsNotInstalled() throws {
        let installed = vm(driver: true, threeD: nil)
        let data = try JSONEncoder().encode(installed)
        XCTAssertTrue(String(decoding: data, as: UTF8.self).contains("\"virtioDisplayDriverInstalled\":true"))
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: data), installed)

        // A vm.json written before the field existed.
        let old = Data("""
        {"id":"win","name":"Windows","displayName":"Windows","backendKind":"hvf-engine",
         "bootMode":"windows-hvf","bundlePath":"/tmp/win.bridgevm","runnerPath":"",
         "launchSpecPath":"","handoffPath":"","sshKeyPath":"","sshUser":"","leasesPath":"",
         "guestName":"win","displayWidth":1280,"displayHeight":800,"installPending":false,
         "diskPath":"/tmp/win.raw"}
        """.utf8)
        let decoded = try JSONDecoder().decode(VMConfig.self, from: old)
        XCTAssertNil(decoded.virtioDisplayDriverInstalled)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(decoded))
        XCTAssertFalse(launch.virtioGpu2d)
        XCTAssertFalse(String(decoding: try JSONEncoder().encode(decoded), as: UTF8.self)
            .contains("virtioDisplayDriverInstalled"), "Untouched VMs keep their vm.json shape")
    }
}

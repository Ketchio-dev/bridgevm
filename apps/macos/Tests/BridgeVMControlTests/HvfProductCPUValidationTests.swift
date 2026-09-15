import XCTest
@testable import BridgeVMControl

final class HvfProductCPUValidationTests: XCTestCase {
    private struct Fixture {
        let root: URL
        let config: HvfEngineConfig

        init() throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("hvf-cpu-validation-" + UUID().uuidString)
            let disk = root.appendingPathComponent("disk.raw")
            let vars = root.appendingPathComponent("vars.fd")
            let helpers = ["scripts/run-hvf-windows-installed-boot.sh",
                           "target/release/examples/hvf_gic_boot_probe"]
            for helper in helpers {
                let url = root.appendingPathComponent(helper)
                try FileManager.default.createDirectory(
                    at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
                try Data([0]).write(to: url)
                try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
            }
            try Data([1]).write(to: disk)
            try Data().write(to: vars)
            let handle = try FileHandle(forWritingTo: vars)
            defer { try? handle.close() }
            // A sparse metadata fixture; no VM process or helper is launched.
            try handle.truncate(atOffset: FirstRunImport.requiredVarsBytes)
            config = HvfEngineConfig(
                targetDiskPath: disk.path, uefiVarsPath: vars.path,
                evidenceDir: root.appendingPathComponent("evidence").path,
                watchdogMs: nil, ramMiB: 4096, smpCpus: 4, clipboardSync: false,
                shareHostDir: nil, shareGuestDir: nil, virtioNet: false,
                virtioGpu3d: false, nvmeBufferedIO: false,
                ctlFilePath: root.appendingPathComponent("control.ctl").path)
        }

        func remove() throws { try FileManager.default.removeItem(at: root) }
    }

    func testReadinessMatchesRuntimeCPUContractWithoutClamping() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        for (cpus, accepted) in [(0, false), (1, true), (64, true), (65, false), (123, false)] {
            var config = fixture.config
            config.smpCpus = cpus
            let report = config.readiness(repoRoot: fixture.root)
            XCTAssertEqual(report.launchReady, accepted, "CPU \(cpus)")
            XCTAssertEqual(report.launchBlockers.contains { $0.code == "cpu-range" }, !accepted)
            XCTAssertEqual(config.smpCpus, cpus, "invalid stored settings must remain visible")
        }
    }

    func testImportValidationMatchesRuntimeCPUContract() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        for (cpus, accepted) in [(0, false), (1, true), (64, true), (65, false), (123, false)] {
            let inputs = FirstRunImport.Inputs(
                displayName: "Windows", diskPath: fixture.config.targetDiskPath,
                varsPath: fixture.config.uefiVarsPath, vtpmStateDir: nil, memMiB: 4096, cpuCount: cpus)
            let expected: FirstRunImport.ValidationError? = accepted
                ? nil : .badResources(memMiB: 4096, cpuCount: cpus)
            XCTAssertEqual(FirstRunImport.validate(inputs), expected, "CPU \(cpus)")
            XCTAssertEqual(inputs.cpuCount, cpus)
        }
    }

    func testResourceErrorsExplainSupportedCPURange() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        var config = fixture.config
        config.smpCpus = 65
        let issue = config.readiness(repoRoot: fixture.root).launchBlockers.first { $0.code == "cpu-range" }
        XCTAssertTrue(issue?.summary.contains("1~64") == true)
        let error = FirstRunImport.ValidationError.badResources(memMiB: 4096, cpuCount: 65)
        XCTAssertTrue(error.description.contains("1~64"))
        XCTAssertTrue(error.description.contains("65"))
    }
}

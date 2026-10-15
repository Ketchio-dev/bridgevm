import XCTest
@testable import BridgeVMControl

final class GenericOwnedLaunchPlanTests: XCTestCase {
    func testFastVZPlanPreservesArgumentsEnvironmentAndLegacyRendering() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        let config = fixture.config
        let backend = FastVZBackend(config)
        let plan = try XCTUnwrap(backend.ownedLaunchPlan())
        let arguments = ["--display", "--display-width", "1440", "--display-height", "900",
                         "--allow-real-vz-start", "--handoff-json", config.handoffPath]
        XCTAssertEqual(plan.primary.executable, fixture.runner.path)
        XCTAssertEqual(plan.primary.arguments, arguments)
        XCTAssertEqual(plan.primary.environmentOverrides, ["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START": "1"])
        XCTAssertEqual(plan.primary.output, .discard)
        XCTAssertTrue(plan.directories.isEmpty)
        XCTAssertNil(plan.tpm)
        XCTAssertEqual(backend.launchCommand(),
            "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 nohup \(Shell.shellCommand(fixture.runner.path, arguments)) >/dev/null 2>&1 &")
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.bundlePath))
    }

    func testFastVZPlanRefusesTheExistingInvalidLaunchInputs() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        for invalid in ["runner", "handoff", "width", "height"] {
            var config = fixture.config
            switch invalid {
            case "runner": config.runnerPath = fixture.missing
            case "handoff": config.handoffPath = fixture.missing
            case "width": config.displayWidth = 0
            default: config.displayHeight = 0
            }
            XCTAssertNil(FastVZBackend(config).ownedLaunchPlan(), invalid)
        }
    }

    func testQemuPlanPreservesDeviceArgumentsAndUnownedTPMReuseDescription() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        var config = fixture.config
        config.memMiB = 8192; config.cpuCount = 6; config.isoPath = fixture.iso.path
        let definition = fixture.qemu(config)
        let plan = try XCTUnwrap(definition.validatedPlan())
        let socket = config.bundlePath + "/metadata/swtpm.sock"
        let state = config.bundlePath + "/metadata/swtpm-state"
        let escaped: (String) -> String = { $0.replacingOccurrences(of: ",", with: ",,") }
        let arguments = ["-name", config.name, "-machine", "virt", "-accel", "hvf", "-cpu", "host",
            "-m", "8192", "-smp", "6", "-bios", fixture.firmware.path,
            "-device", "ramfb", "-device", "qemu-xhci,id=usb", "-device", "usb-kbd,bus=usb.0",
            "-device", "usb-tablet,bus=usb.0", "-drive", "if=none,id=disk,format=qcow2,file=\(escaped(fixture.disk.path))",
            "-device", "nvme,drive=disk,serial=bridgevm", "-netdev", "user,id=net0",
            "-device", "virtio-net-pci,netdev=net0", "-chardev", "socket,id=chrtpm,path=\(escaped(socket))",
            "-tpmdev", "emulator,id=tpm0,chardev=chrtpm", "-device", "tpm-tis-device,tpmdev=tpm0",
            "-display", "cocoa", "-drive", "if=none,id=installer,file=\(escaped(fixture.iso.path)),media=cdrom,readonly=on",
            "-device", "usb-storage,bus=usb.0,drive=installer,bootindex=0"]
        XCTAssertEqual(plan.primary.executable, fixture.qemuExecutable.path)
        XCTAssertEqual(plan.primary.arguments, arguments)
        XCTAssertEqual(plan.primary.environmentOverrides, [:])
        XCTAssertEqual(plan.primary.output, .file(config.bundlePath + "/logs/qemu.log"))
        XCTAssertEqual(plan.directories, [state, config.bundlePath + "/logs"])
        let tpm = try XCTUnwrap(plan.tpm)
        XCTAssertEqual(tpm.reusePattern, socket)
        XCTAssertEqual(tpm.launchDelay, 1.5)
        XCTAssertEqual(tpm.process, .init(executable: fixture.swtpm.path,
            arguments: ["socket", "--tpmstate", "dir=\(state)", "--ctrl", "type=unixio,path=\(socket)", "--tpm2"]))
        XCTAssertTrue(definition.command.contains(Shell.shellCommand(plan.primary.executable, arguments)))
        XCTAssertTrue(definition.command.contains(Shell.shellCommand("/usr/bin/pgrep", ["-f", Shell.eregEscape(socket)])))
        XCTAssertTrue(definition.command.contains("; sleep 1.5; "))
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.bundlePath), "Planning creates no directories or processes")
    }

    func testQemuPlanKeepsDefaultsAndRefusesMissingInputsWithoutPreparation() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        let baseline = try XCTUnwrap(fixture.qemu(fixture.config).validatedPlan())
        XCTAssertTrue(baseline.primary.arguments.contains("6144"))
        XCTAssertTrue(baseline.primary.arguments.contains("4"))
        XCTAssertFalse(baseline.primary.arguments.contains { $0.contains("id=installer") })
        for invalid in ["memory", "cpu", "disk", "iso", "qemu", "firmware", "swtpm"] {
            var config = fixture.config
            if invalid == "memory" { config.memMiB = 0 }
            if invalid == "cpu" { config.cpuCount = 0 }
            if invalid == "disk" { config.diskPath = fixture.missing }
            if invalid == "iso" { config.isoPath = fixture.missing }
            let definition = QemuCompatLaunchDefinition(config: config,
                qemu: invalid == "qemu" ? fixture.missing : fixture.qemuExecutable.path,
                edk2: invalid == "firmware" ? fixture.missing : fixture.firmware.path,
                swtpm: invalid == "swtpm" ? fixture.missing : fixture.swtpm.path)
            XCTAssertNil(definition.validatedPlan(), invalid)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.config.bundlePath))
    }

    private final class Fixture {
        let root: URL
        var runner: URL { root.appendingPathComponent("runner") }
        var qemuExecutable: URL { root.appendingPathComponent("qemu") }
        var swtpm: URL { root.appendingPathComponent("swtpm") }
        var firmware: URL { root.appendingPathComponent("firmware.fd") }
        var disk: URL { root.appendingPathComponent("disk,one.qcow2") }
        var iso: URL { root.appendingPathComponent("installer,one.iso") }
        var missing: String { root.appendingPathComponent("absent").path }
        var config: VMConfig {
            VMConfig(id: "fixture", name: "VM'$(literal)", displayName: "Owned fixture", backendKind: "fast-vz",
                bundlePath: root.appendingPathComponent("bundle'$(literal),one").path,
                runnerPath: runner.path, launchSpecPath: "", handoffPath: firmware.path,
                sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "fixture",
                displayWidth: 1440, displayHeight: 900, diskPath: disk.path)
        }
        init() throws {
            let base = ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory()
            root = URL(fileURLWithPath: base).appendingPathComponent("owned-launch-plan-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            for file in [runner, qemuExecutable, swtpm, firmware, disk, iso] {
                try Data("synthetic launch input".utf8).write(to: file)
            }
            for file in [runner, qemuExecutable, swtpm] {
                try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.path)
            }
        }
        func qemu(_ config: VMConfig) -> QemuCompatLaunchDefinition {
            .init(config: config, qemu: qemuExecutable.path, edk2: firmware.path, swtpm: swtpm.path)
        }
        func clean() { try? FileManager.default.removeItem(at: root) }
    }
}

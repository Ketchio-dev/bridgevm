import Foundation

struct QemuCompatLaunchDefinition {
    let config: VMConfig
    let qemu: String
    let edk2: String
    let swtpm: String
    private var diskPath: String { config.diskPath ?? (config.bundlePath + "/disks/win.qcow2") }
    private var swtpmSock: String { config.bundlePath + "/metadata/swtpm.sock" }
    private var swtpmState: String { config.bundlePath + "/metadata/swtpm-state" }

    private func qemuOptionValue(_ value: String) -> String {
        // QEMU key-value options use doubled commas for a literal path comma.
        value.replacingOccurrences(of: ",", with: ",,")
    }

    var plan: GenericOwnedLaunchPlan {
        let mem = config.memMiB ?? 6144
        let cpu = config.cpuCount ?? 4
        var arguments = ["-name", config.name, "-machine", "virt", "-accel", "hvf", "-cpu", "host",
                         "-m", String(mem), "-smp", String(cpu), "-bios", edk2,
                         "-device", "ramfb", "-device", "qemu-xhci,id=usb",
                         "-device", "usb-kbd,bus=usb.0", "-device", "usb-tablet,bus=usb.0",
                         "-drive", "if=none,id=disk,format=qcow2,file=\(qemuOptionValue(diskPath))",
                         "-device", "nvme,drive=disk,serial=bridgevm", "-netdev", "user,id=net0",
                         "-device", "virtio-net-pci,netdev=net0",
                         "-chardev", "socket,id=chrtpm,path=\(qemuOptionValue(swtpmSock))",
                         "-tpmdev", "emulator,id=tpm0,chardev=chrtpm",
                         "-device", "tpm-tis-device,tpmdev=tpm0", "-display", "cocoa"]
        if let iso = config.isoPath, !iso.isEmpty {
            arguments += ["-drive", "if=none,id=installer,file=\(qemuOptionValue(iso)),media=cdrom,readonly=on",
                          "-device", "usb-storage,bus=usb.0,drive=installer,bootindex=0"]
        }
        let primary = GenericOwnedProcessSpec(executable: qemu, arguments: arguments,
            output: .file(config.bundlePath + "/logs/qemu.log"))
        return .init(primary: primary, directories: [swtpmState, config.bundlePath + "/logs"], tpm: tpm)
    }

    private var tpm: GenericOwnedTPMPreparation {
        .init(process: .init(executable: swtpm,
                             arguments: ["socket", "--tpmstate", "dir=\(swtpmState)",
                                         "--ctrl", "type=unixio,path=\(swtpmSock)", "--tpm2"]),
              reusePattern: swtpmSock)
    }

    func validatedPlan() -> GenericOwnedLaunchPlan? {
        guard (config.memMiB ?? 6144) > 0, (config.cpuCount ?? 4) > 0,
              FileManager.default.isExecutableFile(atPath: qemu),
              FileManager.default.isExecutableFile(atPath: swtpm),
              FileManager.default.isReadableFile(atPath: edk2),
              FileManager.default.isReadableFile(atPath: diskPath) else { return nil }
        if let iso = config.isoPath, !iso.isEmpty {
            guard FileManager.default.isReadableFile(atPath: iso) else { return nil }
        }
        return plan
    }

    var command: String {
        let primary = plan.primary
        let qemuCommand = "nohup \(Shell.shellCommand(primary.executable, primary.arguments)) >\(Shell.shQuote(config.bundlePath + "/logs/qemu.log")) 2>&1 &"
        let mkdir = Shell.shellCommand("/bin/mkdir", ["-p"] + plan.directories)
        let probe = Shell.shellCommand("/usr/bin/pgrep", ["-f", Shell.eregEscape(tpm.reusePattern)])
        let startTPM = "nohup \(Shell.shellCommand(tpm.process.executable, tpm.process.arguments)) >/dev/null 2>&1 &"
        return "\(mkdir); \(probe) >/dev/null 2>&1 || (\(startTPM)); sleep 1.5; \(qemuCommand)"
    }
}

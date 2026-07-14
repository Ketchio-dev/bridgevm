import Foundation

struct HvfEngineConfig: Equatable {
    var targetDiskPath: String
    var uefiVarsPath: String
    var evidenceDir: String
    /// Per-boot diagnostic watchdog. `nil` is the normal app mode: the VM stays
    /// alive until the guest or user requests shutdown.
    var watchdogMs: Int?
    var ramMiB: Int
    var smpCpus: Int
    var clipboardSync: Bool
    var shareHostDir: String?
    var shareGuestDir: String?
    var virtioNet: Bool
    /// Enable the normal Windows display path proven by the live VirGL
    /// evidence run. When disabled, the wrapper retains its legacy 2D device.
    var virtioGpu3d: Bool
    var nvmeBufferedIO: Bool
    var ctlFilePath: String
    /// WinPE driver-injector image booted as NSID 1 for one firstboot cycle;
    /// staged by the install/import flows through the inject-pending marker.
    var placeholderNsid1Path: String?
    /// Enables the wrapper's BOOT_TIMER ramfb lines; the injection flow uses
    /// the `source=virtio-gpu` capture line as its 3D-active confirmation.
    var bootTimerDesktopAgent: Bool = false
    /// Present while a driver injection is pending. The session renames it to
    /// the done marker once the display switches to the 3D scanout.
    var injectPendingMarkerPath: String?

    static func libraryVM(_ config: VMConfig) -> HvfEngineConfig? {
        guard config.engineKind == .hvfEngine else { return nil }
        // A VM whose unattended install has not completed has no bootable disk
        // yet; the detail view routes it to the install panel instead.
        if config.installPending == true { return nil }
        let evidenceDir = config.bundlePath + "/logs/hvf"
        let injection = pendingInjection(bundlePath: config.bundlePath)
        return HvfEngineConfig(
            targetDiskPath: config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw"),
            uefiVarsPath: config.bundlePath + "/metadata/hvf-vars.fd",
            evidenceDir: evidenceDir,
            watchdogMs: nil,
            ramMiB: config.memMiB ?? 6144,
            smpCpus: config.cpuCount ?? 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: true,
            virtioGpu3d: true,
            nvmeBufferedIO: true,
            ctlFilePath: config.bundlePath + "/metadata/hvf.ctl",
            placeholderNsid1Path: injection?.injectorPath,
            bootTimerDesktopAgent: injection != nil,
            injectPendingMarkerPath: injection?.markerPath
        )
    }

    /// Reads the inject-pending marker (first line = injector image path) and
    /// returns it only when both the marker and the injector image exist.
    static func pendingInjection(
        bundlePath: String,
        fileManager: FileManager = .default
    ) -> (markerPath: String, injectorPath: String)? {
        let markerPath = bundlePath + "/" + HvfWindowsInstallPlan.injectPendingMarker
        guard let data = fileManager.contents(atPath: markerPath),
              let firstLine = String(data: data, encoding: .utf8)?
                  .split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: true)
                  .first
        else { return nil }
        let injectorPath = String(firstLine)
        guard fileManager.isReadableFile(atPath: injectorPath) else { return nil }
        return (markerPath, injectorPath)
    }

    func wrapperArguments() -> [String] {
        var args = [
            "scripts/run-hvf-windows-installed-boot.sh",
            "--target", targetDiskPath,
            "--vars", uefiVarsPath,
            "--evidence-dir", evidenceDir
        ]
        if let watchdogMs {
            args.append(contentsOf: ["--watchdog-ms", String(watchdogMs)])
        } else {
            args.append("--no-watchdog")
        }
        args.append(contentsOf: [
            "--ram-mib", String(ramMiB),
            "--smp-cpus", String(smpCpus),
            "--release",
            "--skip-build",
            "--agent-service-control", ctlFilePath,
            "--agent-service-command", "whoami",
            "--display-export-ppm", "\(evidenceDir)/display.ppm",
            "--display-export-ms", "500",
            "--enable-xhci",
            "--input-control", "\(evidenceDir)/input.ctl"
        ])
        if clipboardSync {
            args.append("--agent-clipboard-sync")
        }
        if let host = shareHostDir, let guest = shareGuestDir, !host.isEmpty, !guest.isEmpty {
            args.append(contentsOf: [
                "--agent-share-host", host,
                "--agent-share-guest", guest,
                "--agent-share-ms", "2000",
                "--agent-share-max-kb", "65536"
            ])
        }
        if virtioNet {
            args.append("--virtio-net")
        }
        if nvmeBufferedIO {
            args.append("--nvme-buffered-io")
        }
        if virtioGpu3d {
            args.append(contentsOf: [
                "--virtio-gpu-3d",
                "--virtio-gpu-device-id", "1050",
                "--gpu-trace-protocol", "virgl"
            ])
        }
        if let placeholderNsid1Path {
            args.append(contentsOf: ["--placeholder-nsid1", placeholderNsid1Path])
        }
        if bootTimerDesktopAgent {
            args.append("--boot-timer-desktop-agent")
        }
        return args
    }
}

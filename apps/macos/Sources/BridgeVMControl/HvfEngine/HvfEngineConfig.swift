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
    var ctlFilePath: String

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
            "--agent-service-command", "whoami"
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
        return args
    }
}

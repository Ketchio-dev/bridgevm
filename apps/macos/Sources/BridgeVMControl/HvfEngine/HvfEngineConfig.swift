import Foundation

struct HvfEngineConfig: Equatable {
    var targetDiskPath: String
    var uefiVarsPath: String
    var evidenceDir: String
    var watchdogMs: Int
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
            "--evidence-dir", evidenceDir,
            "--watchdog-ms", String(watchdogMs)
        ]
        if virtioNet {
            args.append("--virtio-net")
        }
        return args
    }

    func environment() -> [String: String] {
        var env = [
            "BRIDGEVM_VIRTIO_CONSOLE": "1",
            "BRIDGEVM_VIRTIO_CONSOLE_TEST": "1",
            "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS": "480000",
            "BRIDGEVM_VIRTIO_CONSOLE_CMDS": "whoami",
            "BRIDGEVM_VIRTIO_CONSOLE_SERVICE": "1",
            "BRIDGEVM_VIRTIO_CONSOLE_CTL": ctlFilePath
        ]
        if clipboardSync {
            env["BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC"] = "1"
        }
        if let host = shareHostDir, let guest = shareGuestDir, !host.isEmpty, !guest.isEmpty {
            env["BRIDGEVM_VIRTIO_CONSOLE_SHARE"] = "\(host)::\(guest)"
            env["BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS"] = "2000"
        }
        return env
    }
}

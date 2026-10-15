import Foundation

/// QEMU + HVF for Windows 11 ARM. Replicates the verified bridgevm-qemu device
/// shape: edk2 firmware, swtpm TPM 2.0, ramfb GOP, qemu-xhci USB HID, and the
/// installer ISO as a bootable USB CD-ROM, in a native cocoa window.
final class QemuCompatBackend: VMOwnedLaunchingBackend {
    let config: VMConfig
    init(_ config: VMConfig) { self.config = config }

    var displayName: String { config.displayName }
    let kind = "qemu-compat"
    let supportsGuestCommands = false
    let supportsPackageInstall = false
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges = false

    // Homebrew installs under /opt/homebrew on Apple Silicon and /usr/local on
    // Intel, so a single fixed prefix makes this engine unusable on one of them.
    // An environment override comes first for installs in neither place.
    static func toolCandidates(_ suffix: String, override: String?) -> [String] {
        var candidates: [String] = []
        if let override, !override.isEmpty {
            candidates.append((override as NSString).expandingTildeInPath)
        }
        candidates.append("/opt/homebrew" + suffix)
        candidates.append("/usr/local" + suffix)
        return candidates
    }

    private static func resolve(_ suffix: String, _ variable: String, isExecutable: Bool) -> String {
        // DEBUG only. A release build that lets an environment variable choose
        // which qemu or swtpm to launch is redirectable from outside the signed
        // bundle, which is exactly what check-release-overrides.sh forbids.
        var override: String?
        #if DEBUG
        override = ProcessInfo.processInfo.environment[variable]
        #endif
        let candidates = toolCandidates(suffix, override: override)
        let fm = FileManager.default
        let found = candidates.first {
            isExecutable ? fm.isExecutableFile(atPath: $0) : fm.isReadableFile(atPath: $0)
        }
        return found ?? candidates[candidates.count - 1]
    }

    private var qemu: String {
        Self.resolve("/bin/qemu-system-aarch64", "BRIDGEVM_QEMU_BINARY", isExecutable: true)
    }
    private var edk2: String {
        Self.resolve(
            "/share/qemu/edk2-aarch64-code.fd", "BRIDGEVM_QEMU_EDK2_CODE", isExecutable: false)
    }
    // swtpm holds the vTPM's sealed state, so it is resolved through the same
    // bundle-first path the HVF engine uses rather than by searching prefixes:
    // anyone who can write to a searched directory would otherwise choose the TPM.
    private var swtpm: String { VTPMStateSecurity.defaultSwtpmCommand() }
    private var diskPath: String { config.diskPath ?? (config.bundlePath + "/disks/win.qcow2") }
    private var swtpmSock: String { config.bundlePath + "/metadata/swtpm.sock" }
    func isRunning() -> Bool { Shell.isProcessRunning(matching: diskPath) }
    func currentIP() -> String? { isRunning() ? "NAT (QEMU)" : nil }

    @discardableResult func start() -> Bool {
        let mem = config.memMiB ?? 6144
        let cpu = config.cpuCount ?? 4
        guard mem > 0, cpu > 0,
              FileManager.default.isExecutableFile(atPath: qemu),
              FileManager.default.isExecutableFile(atPath: swtpm),
              FileManager.default.isReadableFile(atPath: edk2),
              FileManager.default.isReadableFile(atPath: diskPath) else { return false }
        if let iso = config.isoPath, !iso.isEmpty {
            guard FileManager.default.isReadableFile(atPath: iso) else { return false }
        }
        return Shell.launchDetached(launchCommand())
    }

    private var launchDefinition: QemuCompatLaunchDefinition {
        .init(config: config, qemu: qemu, edk2: edk2, swtpm: swtpm)
    }

    func launchCommand() -> String { launchDefinition.command }

    func ownedLaunchPlan() -> GenericOwnedLaunchPlan? { launchDefinition.validatedPlan() }

    func stop() {
        Shell.killProcesses(matching: diskPath)
        Shell.killProcesses(matching: swtpmSock)
    }
    func resources() -> (memMiB: Int, cpu: Int) { (config.memMiB ?? 0, config.cpuCount ?? 0) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        ("Windows 게스트는 SSH 제어를 지원하지 않습니다 (QEMU).", -1)
    }
}

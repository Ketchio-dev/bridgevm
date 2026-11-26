import Foundation

struct HvfLibraryLaunchContext: Equatable {
    let config: VMConfig
    let rootURL: URL
    var e2eHostDiagnosticStopAdmitted: Bool = false


}

extension VMConfig {
    /// The RAM and CPUs a library HVF start uses; vm.json may leave them out.
    var hvfRAMMiB: Int { memMiB ?? 6144 }
    var hvfCPUs: Int { cpuCount ?? 4 }
}
extension HvfEngineConfig {
    /// The 2D virtio-gpu rides on a guest with the signed display driver and 3D off.
    static func virtioGpu2d(for config: VMConfig) -> Bool {
        config.virtioDisplayDriverInstalled == true && config.experimental3DAllowed != true
    }

    static func libraryVM(_ config: VMConfig, rootURL: URL = VMLibrary.root, e2eHostDiagnosticStopAdmitted: Bool = false) -> HvfEngineConfig? {
        guard config.engineKind == .hvfEngine else { return nil }
        // A VM whose unattended install has not completed has no bootable disk
        // yet; the detail view routes it to the install panel instead.
        if config.installPending == true { return nil }
        let evidenceDir = config.bundlePath + "/logs/hvf"
        return HvfEngineConfig(
            targetDiskPath: config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw"),
            uefiVarsPath: config.bundlePath + "/metadata/hvf-vars.fd",
            evidenceDir: evidenceDir,
            watchdogMs: nil,
            ramMiB: config.hvfRAMMiB,
            smpCpus: config.hvfCPUs,
            clipboardSync: config.clipboardSyncEnabled ?? true,
            shareHostDir: config.sharedFolderHostPath,
            shareGuestDir: config.sharedFolderGuestPath,
            virtioNet: config.networkEnabled ?? true,
            virtioGpu3d: config.experimental3DAllowed ?? false,
            virtioGpu2d: Self.virtioGpu2d(for: config),
            nvmeBufferedIO: false,
            durableDiskWrites: config.durableDiskWritesEnabled ?? false,
            microphoneEnabled: config.microphoneEnabled ?? false,
            portForwards: HvfPortForwards.saved(config.portForwards),
            guestResolution: config.guestResolution,
            retinaResolution: config.retinaResolution,
            commandKeyMode: HvfCommandKeyMode(saved: config.commandKeyMode),
            ctlFilePath: config.bundlePath + "/metadata/hvf.ctl",
            vtpmStateDir: config.bundlePath + "/metadata/vtpm",
            swtpmBin: VTPMStateSecurity.defaultSwtpmCommand(),
            vtpmKeyID: config.slug, allowsExperimental3D: config.experimental3DAllowed ?? false,
            libraryContext: HvfLibraryLaunchContext(config: config, rootURL: rootURL, e2eHostDiagnosticStopAdmitted: e2eHostDiagnosticStopAdmitted)
        )
    }
}

extension HvfLibraryLaunchContext {
    var readinessIssues: [HvfWindowsReadinessIssue] {
        var issues: [HvfWindowsReadinessIssue] = []
        if VMRelocationJournal.isPending(config, rootURL: rootURL) {
            issues.append(.init(code: "relocation-pending", scope: .launch,
                summary: "VM 이동 복구가 끝나지 않았습니다. 시작하기 전에 두 번들과 등록 정보를 확인하세요."))
        }
        if HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: rootURL) {
            issues.append(.init(code: "install-cleanup-pending", scope: .launch,
                summary: "Windows installation cleanup is incomplete. Refresh the library. If it’s still blocked, check the saved installation record and avoid running the installer again."))
        }
        return issues
    }
}

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
                summary: "The move has not finished recovering. Check both bundles and the library entry before starting the VM."))
        }
        if HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: rootURL) {
            issues.append(.init(code: "install-cleanup-pending", scope: .launch,
                summary: "Windows 설치 정리가 끝나지 않았습니다. 라이브러리를 새로 고치고, 계속 막히면 보존된 설치 기록을 확인하세요. 다시 설치하지 마세요."))
        }
        return issues
    }
}

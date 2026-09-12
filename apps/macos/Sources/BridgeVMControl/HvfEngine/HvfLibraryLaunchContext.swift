import Foundation

struct HvfLibraryLaunchContext: Equatable {
    let config: VMConfig
    let rootURL: URL

    var readinessIssues: [HvfWindowsReadinessIssue] {
        guard VMRelocationJournal.isPending(config, rootURL: rootURL) else { return [] }
        return [.init(code: "relocation-pending", scope: .launch,
                      summary: "VM relocation recovery is unresolved. Check both bundles and registration before starting.")]
    }
}

extension HvfEngineConfig {
    static func libraryVM(_ config: VMConfig, rootURL: URL = VMLibrary.root) -> HvfEngineConfig? {
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
            ramMiB: config.memMiB ?? 6144,
            smpCpus: config.cpuCount ?? 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: config.networkEnabled ?? true,
            virtioGpu3d: config.experimental3DAllowed ?? true,
            nvmeBufferedIO: false,
            ctlFilePath: config.bundlePath + "/metadata/hvf.ctl",
            vtpmStateDir: config.bundlePath + "/metadata/vtpm",
            swtpmBin: VTPMStateSecurity.defaultSwtpmCommand(),
            vtpmKeyID: config.slug, allowsExperimental3D: config.experimental3DAllowed ?? true,
            libraryContext: HvfLibraryLaunchContext(config: config, rootURL: rootURL)
        )
    }
}

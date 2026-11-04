import Foundation

struct HvfLibraryLaunchContext: Equatable {
    let config: VMConfig
    let rootURL: URL
    var e2eHostDiagnosticStopAdmitted: Bool = false


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
            ramMiB: config.memMiB ?? 6144,
            smpCpus: config.cpuCount ?? 4,
            clipboardSync: config.clipboardSyncEnabled ?? true,
            shareHostDir: config.sharedFolderHostPath,
            shareGuestDir: config.sharedFolderGuestPath,
            virtioNet: config.networkEnabled ?? true,
            virtioGpu3d: config.experimental3DAllowed ?? false,
            virtioGpu2d: Self.virtioGpu2d(for: config),
            nvmeBufferedIO: false,
            durableDiskWrites: config.durableDiskWritesEnabled ?? false,
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

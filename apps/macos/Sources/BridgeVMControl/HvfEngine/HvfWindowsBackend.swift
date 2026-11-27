import Foundation

/// The library's view of a Windows VM: whether its disk is in use and its saved
/// resources. The VM itself runs through HvfEngineSession.
final class HvfWindowsBackend: VMBackend {
    let config: VMConfig
    private let processIsRunning: (String) -> Bool
    private let libraryRoot: URL
    init(
        _ config: VMConfig,
        processIsRunning: @escaping (String) -> Bool = { Shell.isProcessRunning(matching: $0) },
        libraryRoot: URL = VMLibrary.root
    ) {
        self.config = config
        self.processIsRunning = processIsRunning
        self.libraryRoot = libraryRoot
    }

    var targetDiskPath: String { config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw") }
    var uefiVarsPath: String { config.bundlePath + "/metadata/hvf-vars.fd" }
    var evidenceDir: String { config.bundlePath + "/logs/hvf" }
    var ctlFilePath: String { config.bundlePath + "/metadata/hvf.ctl" }

    func isRunning() -> Bool { processIsRunning(targetDiskPath) }
    func diskHelperIsRunning() -> Bool { Shell.isDiskHelperRunning(matching: targetDiskPath) }

    func resources() -> (memMiB: Int, cpu: Int) {
        let launch = makeHvfEngineConfig()
        return (launch.ramMiB, launch.smpCpus)
    }

    func makeHvfEngineConfig() -> HvfEngineConfig {
        HvfEngineConfig.libraryVM(config, rootURL: libraryRoot) ?? HvfEngineConfig(
            targetDiskPath: targetDiskPath,
            uefiVarsPath: uefiVarsPath,
            evidenceDir: evidenceDir,
            watchdogMs: nil,
            ramMiB: config.memMiB ?? 6144,
            smpCpus: config.cpuCount ?? 4,
            clipboardSync: config.clipboardSyncEnabled ?? true,
            shareHostDir: config.sharedFolderHostPath,
            shareGuestDir: config.sharedFolderGuestPath,
            virtioNet: config.networkEnabled ?? true,
            virtioGpu3d: config.experimental3DAllowed ?? false,
            virtioGpu2d: HvfEngineConfig.virtioGpu2d(for: config),
            nvmeBufferedIO: false,
            durableDiskWrites: config.durableDiskWritesEnabled ?? false,
            microphoneEnabled: config.microphoneEnabled ?? false,
            portForwards: HvfPortForwards.saved(config.portForwards),
            guestResolution: config.guestResolution,
            retinaResolution: config.retinaResolution,
            commandKeyMode: HvfCommandKeyMode(saved: config.commandKeyMode),
            ctlFilePath: ctlFilePath, allowsExperimental3D: config.experimental3DAllowed ?? false
        )
    }
}

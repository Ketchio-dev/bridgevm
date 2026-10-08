import Foundation

extension NativeCLICreateWindows {
    static func requireCreated(_ outcome: HVFCreationOutcome?) throws -> VMConfig {
        switch outcome {
        case .created(let config): return config
        case .publishedButUnsynced(let config, _):
            throw NativeCLIError.unavailable("VM files and registration were preserved, but durable saving was not confirmed. Do not recreate; inspect the library entry: \(config.slug)")
        case nil:
            throw NativeCLIError.unavailable("VM creation did not publish a confirmed registration.")
        }
    }

    static func createSavedVM(_ request: NativeCLICreateWindowsOptions,
                                      _ libraryRoot: URL) -> HVFCreationOutcome? {
        VMLibrary.createWindowsHVFInstall(name: request.name, isoPath: request.isoPath,
            diskGiB: request.diskGiB, injectViogpu3d: false, driverPackageDir: nil,
            storageDir: nil, width: request.resolution.width, height: request.resolution.height,
            memMiB: request.memoryMiB, cpuCount: request.cpuCount,
            networkEnabled: request.networkEnabled, libraryRoot: libraryRoot)
    }
}

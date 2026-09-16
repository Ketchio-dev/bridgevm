import Foundation

enum NativeCLICreateWindows {
    typealias Creator = (NativeCLICreateWindowsOptions, URL) -> VMConfig?
    typealias Reader = (URL, String) throws -> VMConfig

    static func create(_ request: NativeCLICreateWindowsOptions, libraryRoot: URL,
                       creator: Creator = createSavedVM,
                       reader: Reader = NativeLibraryReader.readConfig) throws -> NativeCLICreateWindowsOutput {
        guard let created = creator(request, libraryRoot) else {
            throw NativeCLIError.unavailable("VM creation did not publish a confirmed registration.")
        }
        let saved = try reader(libraryRoot, created.slug)
        guard saved == created, saved.backendKind == "hvf-engine", saved.bootMode == "windows-hvf",
              saved.installPending == true, saved.displayName == request.name,
              saved.cpuCount == request.cpuCount, saved.memMiB == request.memoryMiB,
              saved.displayWidth == request.resolution.width, saved.displayHeight == request.resolution.height,
              saved.networkEnabled == request.networkEnabled,
              let pending = HvfWindowsInstallRequest.load(bundlePath: saved.bundlePath),
              pending.diskGiB == request.diskGiB, pending.isoSHA256 != nil else {
            throw NativeCLIError.unavailable("Saved VM registration did not match the accepted creation request.")
        }
        return NativeCLICreateWindowsOutput(id: saved.slug, displayName: saved.displayName,
            backendKind: saved.backendKind, bootMode: saved.bootMode ?? "", installPending: true,
            cpuCount: request.cpuCount, memoryMiB: request.memoryMiB, diskGiB: request.diskGiB,
            resolution: request.resolution.text, networkEnabled: request.networkEnabled,
            configurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: saved))
    }

    private static func createSavedVM(_ request: NativeCLICreateWindowsOptions,
                                      _ libraryRoot: URL) -> VMConfig? {
        VMLibrary.createWindowsHVFInstall(name: request.name, isoPath: request.isoPath,
            diskGiB: request.diskGiB, injectViogpu3d: false, driverPackageDir: nil,
            storageDir: nil, width: request.resolution.width, height: request.resolution.height,
            memMiB: request.memoryMiB, cpuCount: request.cpuCount,
            networkEnabled: request.networkEnabled, libraryRoot: libraryRoot)
    }
}

extension NativeCLI {
    static func executeCreateWindows(_ options: NativeCLIOptions) throws -> Int32 {
        guard case .createWindows(let request) = options.command else {
            throw NativeCLIError.invalid("Expected create-windows command.")
        }
        let result = try NativeCLICreateWindows.create(request, libraryRoot: options.libraryRoot)
        try output(result, json: options.json, text: result.text)
        return 0
    }
}

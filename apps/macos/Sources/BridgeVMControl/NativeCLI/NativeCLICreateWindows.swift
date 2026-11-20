import Foundation

enum NativeCLICreateWindows {
    typealias Creator = (NativeCLICreateWindowsOptions, URL) -> HVFCreationOutcome?
    typealias Reader = (URL, String) throws -> VMConfig

    static func create(_ request: NativeCLICreateWindowsOptions, libraryRoot: URL,
                       creator: Creator = createSavedVM,
                       reader: Reader = NativeLibraryReader.readConfig) throws -> NativeCLICreateWindowsOutput {
        let created = try requireCreated(creator(request, libraryRoot))
        let saved = try reader(libraryRoot, created.slug)
        guard saved == created, saved.backendKind == "hvf-engine", saved.bootMode == "windows-hvf",
              saved.installPending == true, saved.displayName == request.name,
              saved.cpuCount == request.cpuCount, saved.memMiB == request.memoryMiB,
              saved.displayWidth == request.resolution.width, saved.displayHeight == request.resolution.height,
              saved.networkEnabled == request.networkEnabled,
              let pending = HvfWindowsInstallRequest.load(bundlePath: saved.bundlePath),
              pending.diskGiB == request.diskGiB, pending.isoSHA256 != nil,
              (request.guestPayload == nil) == (pending.guestPayloadIdentity == nil) else {
            throw NativeCLIError.unavailable("Saved VM registration did not match the accepted creation request.")
        }
        return NativeCLICreateWindowsOutput(id: saved.slug, displayName: saved.displayName,
            backendKind: saved.backendKind, bootMode: saved.bootMode ?? "", installPending: true,
            cpuCount: request.cpuCount, memoryMiB: request.memoryMiB, diskGiB: request.diskGiB,
            resolution: request.resolution.text, networkEnabled: request.networkEnabled,
            guestDriversSealed: pending.guestPayloadIdentity != nil,
            configurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: saved))
    }


}

extension NativeCLI {
    static func executeCreateWindows(_ options: NativeCLIOptions) throws -> Int32 {
        guard case .createWindows(var request) = options.command else {
            throw NativeCLIError.invalid("Expected create-windows command.")
        }
        if request.fetchDrivers {
            write("BridgeVM: preparing the pinned Red Hat virtio-win drivers (about 95 MB on first use)...\n", to: .standardError)
            do { request.guestPayload = try HvfWindowsDriverPayloadFetch.prepare() } catch {
                throw NativeCLIError.unavailable((error as? HvfWindowsDriverPayloadFetch.Failure)?.message
                    ?? "드라이버를 준비하지 못했습니다. 다시 시도하세요.")
            }
        }
        let result = try NativeCLICreateWindows.create(request, libraryRoot: options.libraryRoot)
        try output(result, json: options.json, text: result.text)
        return 0
    }
}

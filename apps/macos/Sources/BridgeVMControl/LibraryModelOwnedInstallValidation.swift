import Foundation

extension LibraryModel {
    func validatedInstallConfiguration(_ request: NativeInstallControlRequest,
                                       library: NativeRuntimeLibraryHandle) throws -> VMConfig {
        try library.validateCurrentIdentity()
        let matches = vms.filter { $0.slug == request.vmID }
        guard !matches.isEmpty else { throw NativeInstallControlRefusal.targetUnavailable }
        guard matches.count == 1, let current = matches.first else {
            throw NativeInstallControlRefusal.ambiguousTarget
        }
        let saved: VMConfig
        do { saved = try NativeCLIStatusConfigReader.read(library: library, id: request.vmID) }
        catch { throw NativeInstallControlRefusal.configurationChanged }
        guard current == saved,
              try NativeRuntimeConfigurationIdentity.digest(config: saved)
                == request.expectedSavedConfigurationDigest else {
            throw NativeInstallControlRefusal.configurationChanged
        }
        guard saved.engineKind == .hvfEngine, saved.installPending == true else {
            throw NativeInstallControlRefusal.unsupportedTarget
        }
        try library.validateCurrentIdentity()
        return saved
    }
}

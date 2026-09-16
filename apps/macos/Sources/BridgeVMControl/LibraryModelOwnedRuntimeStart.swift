import Foundation

extension LibraryModel {
    func requestOwnedRuntimeStart(_ request: NativeRuntimeStartRequest,
                                 validateOwner: @escaping () throws -> Void,
                                 onReserved: @MainActor (HvfOwnedStartOperation) throws -> Void) throws -> HvfOwnedStartAdmission {
        try NativeRuntimeStartCodec.validate(request)
        guard request.operation == .start, NativeLibraryReader.isCanonicalID(request.vmID),
              let operationID = UUID(uuidString: request.operationID) else { throw NativeRuntimeError.invalidMessage }
        try validateOwner()
        let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
        guard library.identity == request.library else { throw NativeRuntimeStartRefusal.configurationChanged }
        let saved = try validatedStartConfiguration(request, library: library)
        guard saved.engineKind == .hvfEngine, saved.installPending != true,
              let launch = HvfEngineConfig.libraryVM(saved, rootURL: rootURL) else {
            throw NativeRuntimeStartRefusal.unsupportedRuntime
        }
        guard libraryWorkRefusal(slug: saved.slug, owns: { _, current in current == saved }) == nil else {
            throw NativeRuntimeStartRefusal.busy
        }
        guard let session = hvfRuntimeSession(for: saved), hvfRuntimeSessions.owns(session, for: saved) else {
            throw NativeRuntimeStartRefusal.targetUnavailable
        }
        return session.requestOwnedStart(configuration: launch, operationID: operationID,
            expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest, onReserved: onReserved) {
                [weak self, weak session] in
                guard let self, let session, session.matchesRuntimeReservation(operationID),
                      self.hvfRuntimeSessions.owns(session, for: saved) else { return false }
                do {
                    try validateOwner()
                    return try self.validatedStartConfiguration(request, library: library) == saved
                } catch { return false }
            }
    }

    private func validatedStartConfiguration(_ request: NativeRuntimeStartRequest,
                                             library: NativeRuntimeLibraryHandle) throws -> VMConfig {
        try library.validateCurrentIdentity()
        let matches = vms.filter { $0.slug == request.vmID }
        guard !matches.isEmpty else { throw NativeRuntimeStartRefusal.targetUnavailable }
        guard matches.count == 1, let current = matches.first else { throw NativeRuntimeStartRefusal.ambiguousTarget }
        let saved: VMConfig
        do { saved = try NativeCLIStatusConfigReader.read(library: library, id: request.vmID) }
        catch { throw NativeRuntimeStartRefusal.configurationChanged }
        guard current == saved,
              try NativeRuntimeConfigurationIdentity.digest(config: saved) == request.expectedSavedConfigurationDigest else {
            throw NativeRuntimeStartRefusal.configurationChanged
        }
        try library.validateCurrentIdentity()
        return saved
    }
}

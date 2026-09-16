import Darwin
import Foundation

/// Ordinary app admission precedes App.main and all library model effects.
@MainActor
final class NativeRuntimeAppOwner {
    private(set) static var prepared: NativeRuntimeAppOwner?
    private let library: NativeRuntimeLibraryHandle
    private let owner: NativeRuntimeOwner
    private let options: BridgeVMControlLaunchOptions
    private let appInstanceID = UUID().uuidString
    private let cache: NativeRuntimeModelCache<LibraryModel>

    private init(options: BridgeVMControlLaunchOptions) throws {
        self.options = options
        library = try NativeRuntimeLibraryHandle.open(rootURL: options.e2eLibraryRoot ?? VMLibrary.root, create: true)
        owner = try NativeRuntimeOwner(library: library)
        cache = NativeRuntimeModelCache(owner: owner)
    }

    static func prepareOrExit(arguments: [String]) {
        ControlCommandDispatch.validateOrExit(arguments: arguments)
        do {
            let instance = try NativeRuntimeAppOwner(options: BridgeVMControlLaunchOptions.parse(arguments: arguments))
            try instance.owner.start { [weak instance] request in
                guard let instance else { throw NativeRuntimeError.ownerUnavailable }
                return try await instance.observation(request)
            }
            prepared = instance
        } catch {
            let message = (error as? NativeRuntimeError) == .ownerBusy
                ? "This native VM library is already managed by a running BridgeVM app."
                : error.localizedDescription
            FileHandle.standardError.write(Data("BridgeVM: \(message)\n".utf8))
            exit(1)
        }
    }

    func model() -> LibraryModel {
        do {
            return try cache.model {
                LibraryModel(rootURL: URL(fileURLWithPath: library.identity.canonicalPath),
                    e2eUnattendedPath: options.e2eUnattendedPath?.path,
                    migrateLegacy: BridgeVMControlLaunchPolicy.shouldMigrateLegacy(options: options))
            }
        } catch {
            FileHandle.standardError.write(Data("BridgeVM: \(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }

    private func observation(_ request: NativeRuntimeRequest) throws -> NativeRuntimeResponse {
        try Task.checkCancellation()
        try NativeRuntimeCodec.validate(request)
        guard request.library == library.identity, NativeLibraryReader.isCanonicalID(request.vmID),
              let retainedModel = cache.retainedValue else { throw NativeRuntimeError.snapshotUnavailable }
        let sessions = try retainedModel.runtimeObservations(slug: request.vmID,
            requestedConfigurationIdentity: request.savedConfiguration.digest)
        return NativeRuntimeResponse(schema: NativeRuntimeCodec.responseSchema, requestID: request.requestID,
            library: library.identity, vmID: request.vmID, appInstanceID: appInstanceID,
            observedAt: Date().timeIntervalSince1970, scope: NativeRuntimeCodec.scope, sessions: sessions)
    }

    static func shutdown() {
        prepared?.owner.close()
        prepared = nil
    }
}

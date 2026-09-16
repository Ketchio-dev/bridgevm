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
    private lazy var requests = NativeRuntimeAppRequestHandlers(appInstanceID: appInstanceID, library: library.identity,
        validateOwner: { [weak self] in
            guard let self else { throw NativeRuntimeError.ownerUnavailable }
            try self.owner.validateCurrentOwnership()
        }, retainedModel: { [weak self] in self?.cache.retainedValue })

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
            try instance.requests.serve(owner: instance.owner) { [weak instance] request in
                guard let instance else { throw NativeRuntimeError.ownerUnavailable }
                return try await NativeRuntimeAppObservation.response(request, library: instance.library.identity,
                    appInstanceID: instance.appInstanceID, retainedModel: instance.cache.retainedValue)
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

    static func shutdown() {
        prepared?.owner.close()
        prepared = nil
    }
}

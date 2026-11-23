import Foundation

@MainActor
final class NativeRuntimeAppRequestHandlers {
    private let control: NativeRuntimeAppControlRouter; private let start: NativeRuntimeAppStartRouter; private let install: NativeRuntimeAppInstallRouter

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity,
         validateOwner: @escaping () throws -> Void, retainedModel: @escaping () -> LibraryModel?) {
        control = .init(appInstanceID: appInstanceID, library: library, retainedModel: retainedModel)
        start = .init(appInstanceID: appInstanceID, library: library, retainedModel: retainedModel)
        install = .init(appInstanceID: appInstanceID, library: library, validateOwner: validateOwner, retainedModel: retainedModel)
    }

    func serve(owner: NativeRuntimeOwner, status: @escaping NativeRuntimeServer.Handler) throws {
        try owner.start(controlHandler: { [control] request, _ in
            try await control.handle(request)
        }, startHandler: { [start] request, _ in
            try await start.handle(request)
        }, installHandler: { [install] request, _ in
            try await install.handle(request)
        }, handler: status)
    }
}

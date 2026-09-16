import Foundation

@MainActor
final class NativeRuntimeAppRequestHandlers {
    private let control: NativeRuntimeAppControlRouter
    private let start: NativeRuntimeAppStartRouter

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity,
         validateOwner: @escaping () throws -> Void, retainedModel: @escaping () -> LibraryModel?) {
        control = .init(appInstanceID: appInstanceID, library: library,
                        validateOwner: validateOwner, retainedModel: retainedModel)
        start = .init(appInstanceID: appInstanceID, library: library,
                      validateOwner: validateOwner, retainedModel: retainedModel)
    }

    func serve(owner: NativeRuntimeOwner, status: @escaping NativeRuntimeServer.Handler) throws {
        try owner.start(controlHandler: { [control] request, context in
            try await control.handle(request, context: context)
        }, startHandler: { [start] request, context in
            try await start.handle(request, context: context)
        }, handler: status)
    }
}

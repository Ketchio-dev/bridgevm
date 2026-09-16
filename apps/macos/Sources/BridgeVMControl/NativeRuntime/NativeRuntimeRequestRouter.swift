import Foundation

struct NativeRuntimeRequestRouter: Sendable {
    typealias ControlHandler = @Sendable (NativeRuntimeControlRequest, NativeRuntimeRequestContext) async throws -> NativeRuntimeControlResponse
    typealias StartHandler = @Sendable (NativeRuntimeStartRequest, NativeRuntimeRequestContext) async throws -> NativeRuntimeStartResponse
    let library: NativeRuntimeLibraryIdentity
    let status: NativeRuntimeServer.Handler
    let control: ControlHandler?
    let start: StartHandler?

    init(library: NativeRuntimeLibraryIdentity, status: @escaping NativeRuntimeServer.Handler,
         control: ControlHandler? = nil, start: StartHandler? = nil) {
        self.library = library; self.status = status; self.control = control; self.start = start
    }
}

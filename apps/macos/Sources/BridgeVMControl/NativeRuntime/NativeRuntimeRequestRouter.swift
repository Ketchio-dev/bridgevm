import Foundation

struct NativeRuntimeRequestRouter: Sendable {
    typealias ControlHandler = @Sendable (NativeRuntimeControlRequest, NativeRuntimeRequestContext) async throws -> NativeRuntimeControlResponse
    typealias StartHandler = @Sendable (NativeRuntimeStartRequest, NativeRuntimeRequestContext) async throws -> NativeRuntimeStartResponse
    typealias InstallHandler = @Sendable (NativeInstallControlRequest, NativeRuntimeRequestContext) async throws -> NativeInstallControlResponse
    let library: NativeRuntimeLibraryIdentity
    let status: NativeRuntimeServer.Handler
    let control: ControlHandler?; let start: StartHandler?; let install: InstallHandler?

    init(library: NativeRuntimeLibraryIdentity, status: @escaping NativeRuntimeServer.Handler,
         control: ControlHandler? = nil, start: StartHandler? = nil, install: InstallHandler? = nil) {
        self.library = library; self.status = status; self.control = control; self.start = start; self.install = install
    }
}

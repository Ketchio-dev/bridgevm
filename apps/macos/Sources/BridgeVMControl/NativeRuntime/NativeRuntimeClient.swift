import Darwin
import Foundation

enum NativeRuntimeClient {
    static func query(rootURL: URL, request: NativeRuntimeRequest) throws -> NativeRuntimeResponse {
        let deadline = NativeRuntimeTransport.now + NativeRuntimeTransport.timeout
        try NativeRuntimeCodec.validate(request)
        let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
        guard request.library == library.identity else { throw NativeRuntimeError.libraryChanged }
        let endpoint = try NativeRuntimeEndpoint(library: library.identity, create: false)
        guard let identity = try endpoint.socketIdentity() else { throw NativeRuntimeError.ownerUnavailable }
        let fd = try NativeRuntimeTransport.socket()
        defer { Darwin.close(fd) }
        try NativeRuntimeTransport.connect(fd, path: endpoint.socketPath, deadline: deadline)
        try NativeRuntimeTransport.verifyPeer(fd)
        guard try endpoint.socketIdentity() == identity else { throw NativeRuntimeError.invalidEndpoint }
        try library.validateCurrentIdentity()
        try NativeRuntimeTransport.writeFrame(NativeRuntimeCodec.encode(request), to: fd,
            limit: NativeRuntimeCodec.maximumRequestBytes, deadline: deadline)
        let data = try NativeRuntimeTransport.readFrame(fd, limit: NativeRuntimeCodec.maximumResponseBytes,
                                                       deadline: deadline)
        let response = try NativeRuntimeCodec.decode(NativeRuntimeResponse.self, from: data,
                                                     limit: NativeRuntimeCodec.maximumResponseBytes)
        try NativeRuntimeCodec.validate(response, for: request)
        try library.validateCurrentIdentity()
        guard try endpoint.socketIdentity() == identity else { throw NativeRuntimeError.invalidEndpoint }
        try NativeRuntimeTransport.checkDeadline(deadline)
        return response
    }
}

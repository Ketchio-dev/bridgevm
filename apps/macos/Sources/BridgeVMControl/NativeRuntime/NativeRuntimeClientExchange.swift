import Darwin
import Foundation

enum NativeRuntimeClientExchange {
    static func perform<Value>(rootURL: URL, identity expected: NativeRuntimeLibraryIdentity,
                               bytes: Data, decode: (Data) throws -> Value) throws -> Value {
        let deadline = NativeRuntimeTransport.now + NativeRuntimeTransport.timeout
        let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
        guard expected == library.identity else { throw NativeRuntimeError.libraryChanged }
        let endpoint = try NativeRuntimeEndpoint(library: library.identity, create: false)
        guard let identity = try endpoint.socketIdentity() else { throw NativeRuntimeError.ownerUnavailable }
        let fd = try NativeRuntimeTransport.socket()
        defer { Darwin.close(fd) }
        try NativeRuntimeTransport.connect(fd, path: endpoint.socketPath, deadline: deadline)
        try NativeRuntimeTransport.verifyPeer(fd)
        guard try endpoint.socketIdentity() == identity else { throw NativeRuntimeError.invalidEndpoint }
        try library.validateCurrentIdentity()
        try NativeRuntimeTransport.writeFrame(bytes, to: fd,
            limit: NativeRuntimeCodec.maximumRequestBytes, deadline: deadline)
        let data = try NativeRuntimeTransport.readFrame(fd, limit: NativeRuntimeCodec.maximumResponseBytes,
                                                       deadline: deadline)
        let response = try decode(data)
        try library.validateCurrentIdentity()
        guard try endpoint.socketIdentity() == identity else { throw NativeRuntimeError.invalidEndpoint }
        try NativeRuntimeTransport.checkDeadline(deadline)
        return response
    }
}

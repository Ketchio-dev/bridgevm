import Darwin
import Foundation

enum NativeRuntimeTransportFixture {
    static func request(_ library: NativeRuntimeLibraryIdentity) -> NativeRuntimeRequest {
        NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
            requestID: UUID().uuidString, library: library, vmID: "개발-vm",
            savedConfiguration: .init(state: .missing, digest: nil))
    }

    static func response(_ request: NativeRuntimeRequest) -> NativeRuntimeResponse {
        NativeRuntimeResponse(schema: NativeRuntimeCodec.responseSchema, requestID: request.requestID,
            library: request.library, vmID: request.vmID, appInstanceID: UUID().uuidString,
            observedAt: Date().timeIntervalSince1970, scope: NativeRuntimeCodec.scope, sessions: [])
    }

    static func require(_ value: Bool, _ reason: String) throws {
        if !value { throw NSError(domain: "NativeRuntimeContract", code: 1,
                                   userInfo: [NSLocalizedDescriptionKey: reason]) }
    }

    static func refuses(_ reason: String, _ work: () throws -> Void) throws {
        do { try work() } catch { return }
        try require(false, reason)
    }

    static func socketPair() throws -> [Int32] {
        var pair: [Int32] = [-1, -1]
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &pair) == 0 else { throw NativeRuntimeError.transportFailure }
        do { for fd in pair { try NativeRuntimeTransport.configure(fd) } }
        catch { pair.forEach { Darwin.close($0) }; throw error }
        return pair
    }

    static func raw(_ bytes: [UInt8], to fd: Int32) throws {
        let count = bytes.withUnsafeBytes { Darwin.write(fd, $0.baseAddress, $0.count) }
        try require(count == bytes.count, "fixture raw write failed")
    }

    static func runOwner(root: URL, control: URL, mode: String) throws {
        let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: true)
        let owner = try NativeRuntimeOwner(library: library)
        defer { owner.close() }
        try owner.start(controlHandler: NativeRuntimeControlContracts.handler(control: control, mode: mode)) { request in
            if mode == "slow" {
                try Data().write(to: control.appendingPathComponent("started-" + request.requestID), options: .withoutOverwriting)
                try await Task.sleep(nanoseconds: 5_000_000_000)
            }
            return response(request)
        }
        // Represents the model factory: a duplicate must never reach this marker.
        try Data(owner.endpoint.socketPath.utf8).write(to: control.appendingPathComponent("ready"), options: .withoutOverwriting)
        let deadline = NativeRuntimeTransport.now + 15
        while NativeRuntimeTransport.now < deadline {
            if FileManager.default.fileExists(atPath: control.appendingPathComponent("stop").path) { return }
            Thread.sleep(forTimeInterval: 0.01)
        }
        throw NativeRuntimeError.timedOut
    }
}

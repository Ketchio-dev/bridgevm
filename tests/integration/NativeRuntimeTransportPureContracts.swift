import Darwin
import Foundation

enum NativeRuntimeTransportPureContracts {
    static func run(root: URL) throws {
        let require = NativeRuntimeTransportFixture.require
        let refuses = NativeRuntimeTransportFixture.refuses
        let missing = root.appendingPathComponent("missing/library")
        try refuses("read-only open created a missing library") {
            _ = try NativeRuntimeLibraryHandle.open(rootURL: missing, create: false)
        }
        try require(!FileManager.default.fileExists(atPath: root.appendingPathComponent("missing").path), "read created dirs")
        let libraryURL = root.appendingPathComponent("library")
        let library = try NativeRuntimeLibraryHandle.open(rootURL: libraryURL, create: true)
        let alias = root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: libraryURL)
        let other = try NativeRuntimeLibraryHandle.open(rootURL: alias, create: false)
        try require(other.identity == library.identity, "aliases differ")
        try require(other.identity.namespaceDigest == library.identity.namespaceDigest, "alias namespaces differ")
        let replacement = root.appendingPathComponent("moved")
        try FileManager.default.moveItem(at: libraryURL, to: replacement)
        try FileManager.default.createDirectory(at: libraryURL, withIntermediateDirectories: false)
        try refuses("replaced library accepted") { try library.validateCurrentIdentity() }
        let pair = try NativeRuntimeTransportFixture.socketPair()
        defer { pair.forEach { Darwin.close($0) } }
        try NativeRuntimeTransport.verifyPeer(pair[0])
        try refuses("wrong peer uid accepted") {
            try NativeRuntimeTransport.verifyPeer(pair[0], expectedUID: geteuid() &+ 1)
        }
        let bytes = Data("bounded frame".utf8)
        try NativeRuntimeTransport.writeFrame(bytes, to: pair[0], limit: 64, deadline: NativeRuntimeTransport.now + 1)
        let read = try NativeRuntimeTransport.readFrame(pair[1], limit: 64, deadline: NativeRuntimeTransport.now + 1)
        try require(read == bytes, "frame changed")
        try refuses("already-ready descriptor bypassed expired deadline") {
            try NativeRuntimeTransport.wait(pair[0], events: Int16(POLLOUT), deadline: NativeRuntimeTransport.now - 1)
        }
        let began = NativeRuntimeTransport.now
        try refuses("partial frame did not time out") {
            try NativeRuntimeTransportFixture.raw([0, 0], to: pair[0])
            _ = try NativeRuntimeTransport.readFrame(pair[1], limit: 64, deadline: NativeRuntimeTransport.now + 0.08)
        }
        try require(NativeRuntimeTransport.now - began < 0.5, "partial-read deadline exceeded")
        let oversized = try NativeRuntimeTransportFixture.socketPair()
        defer { oversized.forEach { Darwin.close($0) } }
        try NativeRuntimeTransportFixture.raw([0, 1, 0, 1], to: oversized[0])
        try refuses("over-limit frame accepted") {
            _ = try NativeRuntimeTransport.readFrame(oversized[1], limit: 65_536, deadline: NativeRuntimeTransport.now + 1)
        }
        let request = NativeRuntimeTransportFixture.request(other.identity)
        let response = NativeRuntimeTransportFixture.response(request)
        let encoded = try NativeRuntimeCodec.encode(response)
        let decoded = try NativeRuntimeCodec.decode(NativeRuntimeResponse.self, from: encoded,
                                                     limit: NativeRuntimeCodec.maximumResponseBytes)
        try NativeRuntimeCodec.validate(decoded, for: request)
        for suffix in [",\"unknown\":true}", ",\"vmID\":\"개발-vm\"}"] {
            let invalid = encoded.dropLast() + Data(suffix.utf8)
            try refuses("unknown/duplicate response key accepted") {
                _ = try NativeRuntimeCodec.decode(NativeRuntimeResponse.self, from: invalid,
                                                   limit: NativeRuntimeCodec.maximumResponseBytes)
            }
        }
        try require(!FileManager.default.fileExists(atPath: missing.path), "read-only query changed library")
    }
}

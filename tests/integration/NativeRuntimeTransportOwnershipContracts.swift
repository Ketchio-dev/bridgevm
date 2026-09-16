import Darwin
import Foundation

enum NativeRuntimeTransportOwnershipContracts {
    static func run(root: URL) throws {
        let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false)
        let endpoint = try NativeRuntimeEndpoint(library: library.identity, create: true)
        func descriptorCount() -> Int { (0..<1024).reduce(0) { $0 + (fcntl(Int32($1), F_GETFD) >= 0 ? 1 : 0) } }
        func cycle() throws {
            let owner = try NativeRuntimeOwner(library: library)
            try owner.start { NativeRuntimeTransportFixture.response($0) }
            owner.close()
            var pipes: [Int32] = [-1, -1]
            guard pipe(&pipes) == 0 else { throw NativeRuntimeError.transportFailure }
            defer { pipes.forEach { Darwin.close($0) } }
            // Keep fresh descriptors active while the dispatch cancellation handler runs.
            Thread.sleep(forTimeInterval: 0.002)
            var sent: UInt8 = 73, received: UInt8 = 0
            try NativeRuntimeTransportFixture.require(Darwin.write(pipes[1], &sent, 1) == 1, "cancellation closed unrelated writer")
            try NativeRuntimeTransportFixture.require(Darwin.read(pipes[0], &received, 1) == 1 && received == sent,
                                                        "cancellation closed unrelated reader")
        }
        try cycle()
        Thread.sleep(forTimeInterval: 0.03)
        let before = descriptorCount()
        for _ in 0..<64 { try cycle() }
        let deadline = NativeRuntimeTransport.now + 1
        while descriptorCount() > before && NativeRuntimeTransport.now < deadline {
            Thread.sleep(forTimeInterval: 0.005)
        }
        try NativeRuntimeTransportFixture.require(descriptorCount() <= before, "listener cancellation failed to release descriptors")
        try NativeRuntimeTransportFixture.require(try endpoint.socketIdentity() == nil, "closed owner left a socket")
        var info = stat()
        try NativeRuntimeTransportFixture.require(lstat(endpoint.lockPath, &info) == 0 && info.st_mode & S_IFMT == S_IFREG,
                                                   "close removed permanent owner lock")
    }
}

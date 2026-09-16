import Darwin
import Foundation

final class NativeRuntimeServer: @unchecked Sendable {
    typealias Handler = @Sendable (NativeRuntimeRequest) async throws -> NativeRuntimeResponse
    private let library: NativeRuntimeLibraryHandle
    private let endpoint: NativeRuntimeEndpoint
    private let router: NativeRuntimeRequestRouter
    private let validateOwner: @Sendable () throws -> Void
    private let descriptor: Int32
    private let socketIdentity: NativeRuntimeFileIdentity
    private let queue = DispatchQueue(label: "dev.bridgevm.native-runtime.accept")
    private let queueKey = DispatchSpecificKey<Bool>()
    private var source: DispatchSourceRead?
    private var clients: [UUID: NativeRuntimeTransportConnection] = [:]
    private var ioFinished: Set<UUID> = []
    private var handlerFinished: Set<UUID> = []
    private var closed = false

    init(library: NativeRuntimeLibraryHandle, endpoint: NativeRuntimeEndpoint,
         validateOwner: @escaping @Sendable () throws -> Void,
         controlHandler: NativeRuntimeRequestRouter.ControlHandler? = nil, startHandler: NativeRuntimeRequestRouter.StartHandler? = nil,
         installHandler: NativeRuntimeRequestRouter.InstallHandler? = nil, handler: @escaping Handler) throws {
        try library.validateCurrentIdentity()
        try endpoint.validate()
        if let stale = try endpoint.socketIdentity() { try endpoint.removeSocket(ifIdentity: stale) }
        let fd = try NativeRuntimeTransport.socket()
        var bound: NativeRuntimeFileIdentity?
        do {
            guard try NativeRuntimeTransport.withAddress(endpoint.socketPath, { Darwin.bind(fd, $0, $1) }) == 0,
                  chmod(endpoint.socketPath, 0o600) == 0 else { throw NativeRuntimeError.invalidEndpoint }
            bound = try endpoint.socketIdentity()
            guard let identity = bound, listen(fd, 4) == 0 else { throw NativeRuntimeError.invalidEndpoint }
            self.library = library; self.endpoint = endpoint; self.validateOwner = validateOwner
            router = .init(library: library.identity, status: handler, control: controlHandler, start: startHandler, install: installHandler)
            descriptor = fd; socketIdentity = identity
        } catch {
            Darwin.close(fd)
            if let bound { try? endpoint.removeSocket(ifIdentity: bound) }
            throw error
        }
        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: queue)
        queue.setSpecific(key: queueKey, value: true)
        source.setEventHandler { [weak self] in self?.acceptReady() }
        source.setCancelHandler { Darwin.close(fd) }
        self.source = source
        source.resume()
    }
    private func acceptReady() {
        guard !closed else { return }
        // Bound this callback as well as the number of live requests.
        for _ in 0..<8 {
            let fd = accept(descriptor, nil, nil)
            guard fd >= 0 else { return }
            guard clients.count < 4 else { Darwin.close(fd); continue }
            do { try NativeRuntimeTransport.configure(fd); try NativeRuntimeTransport.verifyPeer(fd) }
            catch { Darwin.close(fd); continue }
            let id = UUID(), connection = NativeRuntimeTransportConnection(descriptor: fd)
            clients[id] = connection
            DispatchQueue.global(qos: .userInitiated).async { [self] in serve(id, connection: connection) }
        }
    }

    private func serve(_ id: UUID, connection: NativeRuntimeTransportConnection) {
        var startedHandler = false
        defer {
            connection.closeIO()
            let hadHandler = startedHandler
            queue.async { [self] in
                ioFinished.insert(id)
                if !hadHandler { handlerFinished.insert(id) }
                releaseIfFinished(id)
            }
        }
        do {
            try validateOwner()
            let bytes = try NativeRuntimeTransport.readFrame(connection.descriptor,
                limit: NativeRuntimeCodec.maximumRequestBytes, deadline: connection.deadline)
            try NativeRuntimeTransport.checkDeadline(connection.deadline)
            startedHandler = true
            connection.begin(work: { [router] in
                try await router.reply(to: bytes, context: .init(deadline: connection.deadline))
            }) { [weak self] in
                self?.queue.async { [weak self] in
                    self?.handlerFinished.insert(id); self?.releaseIfFinished(id)
                }
            }
            let data = try connection.responseData()
            try validateOwner()
            try NativeRuntimeTransport.writeFrame(data, to: connection.descriptor,
                limit: NativeRuntimeCodec.maximumResponseBytes, deadline: connection.deadline)
        } catch { /* Failed exchanges never become successful or partial wire responses. */ }
    }

    private func releaseIfFinished(_ id: UUID) {
        guard ioFinished.contains(id), handlerFinished.contains(id) else { return }
        clients.removeValue(forKey: id); ioFinished.remove(id); handlerFinished.remove(id)
    }

    func close() {
        if DispatchQueue.getSpecific(key: queueKey) == true { closeOnQueue() }
        else { queue.sync { closeOnQueue() } }
    }

    private func closeOnQueue() {
        guard !closed else { return }
        closed = true
        source?.cancel(); source = nil
        clients.values.forEach { $0.stop() }
        try? endpoint.removeSocket(ifIdentity: socketIdentity)
    }

    deinit { close() }
}

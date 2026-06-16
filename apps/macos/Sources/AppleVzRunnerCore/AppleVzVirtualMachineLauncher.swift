import Foundation

#if canImport(Darwin)
import Darwin
#endif

#if canImport(Virtualization)
import Virtualization
#endif

public enum AppleVzVirtualMachineLaunchResult: Equatable {
  case stopped
  case interrupted
}

public struct AppleVzLaunchOptions: Equatable {
  public var stopAfterSeconds: UInt64?
  public var forceStopGraceSeconds: UInt64?

  public init(stopAfterSeconds: UInt64? = nil, forceStopGraceSeconds: UInt64? = nil) {
    self.stopAfterSeconds = stopAfterSeconds
    self.forceStopGraceSeconds = forceStopGraceSeconds
  }
}

public protocol AppleVzVirtualMachineControlling: AnyObject {
  var canRequestStop: Bool { get }
  var canStop: Bool { get }

  func start(completionHandler: @escaping (Result<Void, Error>) -> Void)
  func requestStop() throws
  func stop(completionHandler: @escaping (Result<Void, Error>) -> Void)
  func setStopHandler(_ handler: @escaping (Result<Void, Error>) -> Void)
}

public final class AppleVzVirtualMachineLauncher {
  private let machine: AppleVzVirtualMachineControlling
  private let clockSleep: (UInt64) async -> Void

  public init(
    machine: AppleVzVirtualMachineControlling,
    clockSleep: @escaping (UInt64) async -> Void = { seconds in
      try? await Task.sleep(for: .seconds(seconds))
    }
  ) {
    self.machine = machine
    self.clockSleep = clockSleep
  }

  public func startAndWaitForStop(
    interruptionSignals: AsyncStream<Void> = AppleVzSignalSource.sigint(),
    forceStopGraceSeconds: UInt64? = nil
  ) async throws -> AppleVzVirtualMachineLaunchResult {
    var wasInterrupted = false
    var didScheduleForceStop = false
    var forceStopTask: Task<Void, Never>?
    let eventSink = LaunchEventSink()
    defer {
      forceStopTask?.cancel()
    }
    let events = AsyncStream<LaunchEvent> { continuation in
      eventSink.setContinuation(continuation)
      machine.setStopHandler { result in
        log("VM stop handler fired: \(result)")
        continuation.yield(.stopped(result))
        continuation.finish()
      }

      log("VM start requested")
      machine.start { result in
        log("VM start completion: \(result)")
        continuation.yield(.started(result))
        if case .failure = result {
          continuation.finish()
        }
      }

      let interruptionTask = Task {
        var iterator = interruptionSignals.makeAsyncIterator()
        if await iterator.next() != nil {
          continuation.yield(.interrupted)
        }
      }

      continuation.onTermination = { _ in
        interruptionTask.cancel()
      }
    }

    for await event in events {
      switch event {
      case .started(let result):
        try result.get()
      case .interrupted:
        log("VM interruption requested")
        wasInterrupted = true
        let requestedGuestStop = try await stopAfterInterruption()
        if requestedGuestStop, !didScheduleForceStop, let forceStopGraceSeconds {
          didScheduleForceStop = true
          log("VM force stop scheduled after \(forceStopGraceSeconds)s")
          forceStopTask = Task {
            await clockSleep(forceStopGraceSeconds)
            if !Task.isCancelled {
              log("VM force stop grace elapsed")
              do {
                try await forceStop()
                eventSink.yield(.stopped(.success(())))
              } catch {
                eventSink.yield(.stopped(.failure(error)))
              }
              eventSink.finish()
            }
          }
        }
      case .stopped(let result):
        forceStopTask?.cancel()
        try result.get()
        return wasInterrupted ? .interrupted : .stopped
      }
    }

    return wasInterrupted ? .interrupted : .stopped
  }

  private func stopAfterInterruption() async throws -> Bool {
    if machine.canRequestStop {
      log("VM guest stop requested")
      try machine.requestStop()
      return true
    }

    guard machine.canStop else {
      log("VM cannot request guest stop or immediate force stop yet")
      return true
    }

    log("VM force stop requested immediately")
    try await forceStop()
    return false
  }

  private func forceStop() async throws {
    log("VM force stop requested")
    try await withCheckedThrowingContinuation { continuation in
      machine.stop { result in
        log("VM force stop completion: \(result)")
        continuation.resume(with: result)
      }
    }
  }
}

private func log(_ message: String) {
  let line = "AppleVzRunner: \(message)\n"
  if let data = line.data(using: .utf8) {
    FileHandle.standardError.write(data)
  }
}

private final class LaunchEventSink {
  private let lock = NSLock()
  private var continuation: AsyncStream<LaunchEvent>.Continuation?

  func setContinuation(_ continuation: AsyncStream<LaunchEvent>.Continuation) {
    lock.withLock {
      self.continuation = continuation
    }
  }

  func yield(_ event: LaunchEvent) {
    _ = lock.withLock {
      continuation?.yield(event)
    }
  }

  func finish() {
    lock.withLock {
      continuation?.finish()
    }
  }
}

private final class MergedSignalSink: @unchecked Sendable {
  private let lock = NSLock()
  private let continuation: AsyncStream<Void>.Continuation
  private var didYield = false
  private var tasks: [Task<Void, Never>] = []

  init(continuation: AsyncStream<Void>.Continuation) {
    self.continuation = continuation
  }

  func setTasks(_ tasks: [Task<Void, Never>]) {
    let shouldCancel = lock.withLock {
      self.tasks = tasks
      return didYield
    }
    if shouldCancel {
      cancelTasks(tasks)
    }
  }

  func yieldAndFinish() {
    let tasksToCancel = lock.withLock {
      guard !didYield else {
        return [Task<Void, Never>]()
      }
      didYield = true
      return tasks
    }
    continuation.yield()
    continuation.finish()
    cancelTasks(tasksToCancel)
  }

  func cancel() {
    let tasksToCancel = lock.withLock {
      tasks
    }
    cancelTasks(tasksToCancel)
  }

  private func cancelTasks(_ tasks: [Task<Void, Never>]) {
    for task in tasks {
      task.cancel()
    }
  }
}

private enum LaunchEvent {
  case started(Result<Void, Error>)
  case interrupted
  case stopped(Result<Void, Error>)
}

public enum AppleVzSignalSource {
  public static func sigint() -> AsyncStream<Void> {
    #if canImport(Darwin)
    AsyncStream { continuation in
      let source = DispatchSource.makeSignalSource(signal: SIGINT, queue: .global())
      let previousHandler = signal(SIGINT, SIG_IGN)
      source.setEventHandler {
        continuation.yield()
      }
      source.setCancelHandler {
        signal(SIGINT, previousHandler)
      }
      continuation.onTermination = { _ in
        source.cancel()
      }
      source.resume()
    }
    #else
    AsyncStream { continuation in
      continuation.finish()
    }
    #endif
  }

  public static func timer(afterSeconds seconds: UInt64?) -> AsyncStream<Void> {
    guard let seconds else {
      return AsyncStream { _ in }
    }

    return AsyncStream { continuation in
      let task = Task {
        try? await Task.sleep(for: .seconds(seconds))
        guard !Task.isCancelled else {
          return
        }
        continuation.yield()
        continuation.finish()
      }
      continuation.onTermination = { _ in
        task.cancel()
      }
    }
  }

  public static func merged(_ streams: [AsyncStream<Void>]) -> AsyncStream<Void> {
    AsyncStream { continuation in
      let sink = MergedSignalSink(continuation: continuation)
      let tasks = streams.map { stream in
        Task {
          var iterator = stream.makeAsyncIterator()
          if await iterator.next() != nil {
            sink.yieldAndFinish()
          }
        }
      }
      sink.setTasks(tasks)
      continuation.onTermination = { _ in
        sink.cancel()
      }
    }
  }
}

#if canImport(Virtualization)
@available(macOS 12.0, *)
public extension AppleVzVirtualMachineLauncher {
  static func launchLinuxKernelVirtualMachine(
    spec: AppleVzLaunchSpec,
    options: AppleVzLaunchOptions = AppleVzLaunchOptions()
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    try configuration.validate()

    print("AppleVzRunner starting VM: \(spec.vmName)")
    let machine = AppleVzVirtualMachineAdapter(configuration: configuration)
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)
    let semaphore = DispatchSemaphore(value: 0)
    var launchResult: Result<AppleVzVirtualMachineLaunchResult, Error>?

    Task {
      do {
        launchResult = .success(
          try await launcher.startAndWaitForStop(
            interruptionSignals: AppleVzSignalSource.merged([
              AppleVzSignalSource.sigint(),
              AppleVzSignalSource.timer(afterSeconds: options.stopAfterSeconds),
            ]),
            forceStopGraceSeconds: options.forceStopGraceSeconds
          )
        )
      } catch {
        launchResult = .failure(error)
      }
      semaphore.signal()
    }

    semaphore.wait()
    let result = try launchResult?.get()
    print("AppleVzRunner VM finished: \(spec.vmName) (\(result ?? .stopped))")
  }
}

@available(macOS 12.0, *)
public final class AppleVzVirtualMachineAdapter: NSObject, AppleVzVirtualMachineControlling,
  VZVirtualMachineDelegate
{
  private let virtualMachine: VZVirtualMachine
  private let queue: DispatchQueue
  private let queueKey = DispatchSpecificKey<Void>()
  private var stopHandler: ((Result<Void, Error>) -> Void)?

  public init(configuration: VZVirtualMachineConfiguration) {
    let queue = DispatchQueue(label: "com.bridgevm.apple-vz.virtual-machine")
    self.queue = queue
    self.queue.setSpecific(key: queueKey, value: ())
    self.virtualMachine = VZVirtualMachine(configuration: configuration, queue: queue)
    super.init()
    self.virtualMachine.delegate = self
  }

  public var canRequestStop: Bool {
    onQueue {
      virtualMachine.canRequestStop
    }
  }

  public var canStop: Bool {
    onQueue {
      virtualMachine.canStop
    }
  }

  public func start(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.start(completionHandler: completionHandler)
    }
  }

  public func requestStop() throws {
    try onQueue {
      try virtualMachine.requestStop()
    }
  }

  public func stop(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.stop { error in
        if let error {
          completionHandler(.failure(error))
        } else {
          completionHandler(.success(()))
        }
      }
    }
  }

  public func setStopHandler(_ handler: @escaping (Result<Void, Error>) -> Void) {
    onQueue {
      stopHandler = handler
    }
  }

  public func guestDidStop(_ virtualMachine: VZVirtualMachine) {
    onQueue {
      stopHandler?(.success(()))
    }
  }

  public func virtualMachine(_ virtualMachine: VZVirtualMachine, didStopWithError error: Error) {
    onQueue {
      stopHandler?(.failure(error))
    }
  }

  private func onQueue<T>(_ operation: () throws -> T) rethrows -> T {
    if DispatchQueue.getSpecific(key: queueKey) != nil {
      return try operation()
    }
    return try queue.sync {
      try operation()
    }
  }
}

public enum AppleVzVirtualMachineAdapterError: Error, LocalizedError, Equatable {
  case stopRequestRejected

  public var errorDescription: String? {
    switch self {
    case .stopRequestRejected:
      return "AppleVzRunner could not request the guest to stop"
    }
  }
}
#endif

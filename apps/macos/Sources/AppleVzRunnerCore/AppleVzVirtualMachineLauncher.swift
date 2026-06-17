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
  /// When set, the VM is paused and its full machine state is written to this
  /// path after `stopAfterSeconds`, instead of being stopped (suspend).
  public var saveStatePath: String?
  /// When set, the VM is restored from this saved state and resumed instead of
  /// performing a fresh boot (resume).
  public var restoreStatePath: String?
  /// When true, boot with a graphical display and host the VM in an on-screen
  /// `VZVirtualMachineView` window (the embedded display). Mutually exclusive
  /// with save/restore (a VZ VM with a graphics device cannot be saved).
  public var displayWindow: Bool
  /// When true, boot the VM with the graphics-device configuration but WITHOUT a
  /// window, on the headless launcher. This is a verification path: it proves a
  /// real guest boots with the Virtio GPU attached (observable on the serial
  /// console) on a host with no window server, which the windowed `displayWindow`
  /// path cannot do.
  public var graphicsHeadlessVerification: Bool
  /// Host directory shared into the guest over VZ-native Virtio FS, plus its
  /// mount tag. The guest mounts it with `mount -t virtiofs <tag> <dir>`.
  public var sharedDirectoryPath: String?
  public var sharedDirectoryTag: String?
  public var sharedDirectoryReadOnly: Bool

  public init(
    stopAfterSeconds: UInt64? = nil,
    forceStopGraceSeconds: UInt64? = nil,
    saveStatePath: String? = nil,
    restoreStatePath: String? = nil,
    displayWindow: Bool = false,
    graphicsHeadlessVerification: Bool = false,
    sharedDirectoryPath: String? = nil,
    sharedDirectoryTag: String? = nil,
    sharedDirectoryReadOnly: Bool = false
  ) {
    self.stopAfterSeconds = stopAfterSeconds
    self.forceStopGraceSeconds = forceStopGraceSeconds
    self.saveStatePath = saveStatePath
    self.restoreStatePath = restoreStatePath
    self.displayWindow = displayWindow
    self.graphicsHeadlessVerification = graphicsHeadlessVerification
    self.sharedDirectoryPath = sharedDirectoryPath
    self.sharedDirectoryTag = sharedDirectoryTag
    self.sharedDirectoryReadOnly = sharedDirectoryReadOnly
  }

  /// The shared-directory spec assembled from the path/tag options, if a path
  /// was provided (tag defaults to "share").
  var sharedDirectorySpec: AppleVzSharedDirectorySpec? {
    #if canImport(Virtualization)
    guard let path = sharedDirectoryPath else { return nil }
    return AppleVzSharedDirectorySpec(
      path: path,
      tag: sharedDirectoryTag ?? "share",
      readOnly: sharedDirectoryReadOnly
    )
    #else
    return nil
    #endif
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
    startMachine: Bool = true,
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

      if startMachine {
        log("VM start requested")
        machine.start { result in
          log("VM start completion: \(result)")
          continuation.yield(.started(result))
          if case .failure = result {
            continuation.finish()
          }
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
    let configuration: VZVirtualMachineConfiguration
    if options.graphicsHeadlessVerification {
      // Verification path: boot the graphics-device config headless (no window)
      // to prove a real guest comes up with the Virtio GPU attached.
      guard #available(macOS 14.0, *) else {
        throw AppleVzRunnerCommandError.displayRequiresMacOS14
      }
      configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay(
        spec: spec, sharedDirectory: options.sharedDirectorySpec)
    } else {
      configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(
        spec: spec, sharedDirectory: options.sharedDirectorySpec)
    }
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

  /// Boot the Linux VM, run for `afterSeconds`, then pause and write the full
  /// machine state (memory + devices) to `statePath` for later resume.
  @available(macOS 14.0, *)
  static func suspendLinuxKernelVirtualMachine(
    spec: AppleVzLaunchSpec,
    afterSeconds: UInt64,
    toStatePath statePath: String
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    try configuration.validate()
    try assertSaveRestoreSupported(configuration)

    print("AppleVzRunner starting VM to suspend: \(spec.vmName)")
    let machine = AppleVzVirtualMachineAdapter(configuration: configuration)
    let semaphore = DispatchSemaphore(value: 0)
    var flowResult: Result<Void, Error>?

    Task {
      do {
        try await awaitVZVoid { machine.start(completionHandler: $0) }
        print("AppleVzRunner VM running; will suspend after \(afterSeconds)s")
        try? await Task.sleep(for: .seconds(afterSeconds))
        try await awaitVZVoid { machine.pause(completionHandler: $0) }
        print("AppleVzRunner VM paused; saving machine state to \(statePath)")
        try await awaitVZVoid {
          machine.saveState(to: URL(fileURLWithPath: statePath), completionHandler: $0)
        }
        print("AppleVzRunner saved machine state: \(statePath)")
        try? await awaitVZVoid { machine.stop(completionHandler: $0) }
        flowResult = .success(())
      } catch {
        // Best-effort cleanup: stop the still-running VM and remove any partial
        // state file so a later --restore-state cannot consume a corrupt save.
        try? await awaitVZVoid { machine.stop(completionHandler: $0) }
        try? FileManager.default.removeItem(at: URL(fileURLWithPath: statePath))
        flowResult = .failure(error)
      }
      semaphore.signal()
    }

    semaphore.wait()
    try flowResult?.get()
    print("AppleVzRunner suspend complete: \(spec.vmName)")
  }

  /// Restore the Linux VM from a previously saved state file and resume it,
  /// running until `options.stopAfterSeconds` elapses (or SIGINT).
  @available(macOS 14.0, *)
  static func restoreLinuxKernelVirtualMachine(
    spec: AppleVzLaunchSpec,
    fromStatePath statePath: String,
    options: AppleVzLaunchOptions = AppleVzLaunchOptions()
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    try configuration.validate()
    try assertSaveRestoreSupported(configuration)

    print("AppleVzRunner restoring VM: \(spec.vmName) from \(statePath)")
    let machine = AppleVzVirtualMachineAdapter(configuration: configuration)
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)
    let semaphore = DispatchSemaphore(value: 0)
    var flowResult: Result<Void, Error>?

    Task {
      do {
        try await awaitVZVoid {
          machine.restoreState(from: URL(fileURLWithPath: statePath), completionHandler: $0)
        }
        print("AppleVzRunner restored machine state; resuming")
        try await awaitVZVoid { machine.resume(completionHandler: $0) }
        print("AppleVzRunner VM resumed: \(spec.vmName)")
        // Reuse the robust wait loop (guest-stop handler + interruption + force
        // stop grace) without re-starting the already-resumed machine.
        _ = try await launcher.startAndWaitForStop(
          startMachine: false,
          interruptionSignals: AppleVzSignalSource.merged([
            AppleVzSignalSource.sigint(),
            AppleVzSignalSource.timer(afterSeconds: options.stopAfterSeconds),
          ]),
          forceStopGraceSeconds: options.forceStopGraceSeconds
        )
        flowResult = .success(())
      } catch {
        try? await awaitVZVoid { machine.stop(completionHandler: $0) }
        flowResult = .failure(error)
      }
      semaphore.signal()
    }

    semaphore.wait()
    try flowResult?.get()
    print("AppleVzRunner restore complete: \(spec.vmName)")
  }
}

@available(macOS 12.0, *)
fileprivate func awaitVZVoid(
  _ body: (@escaping (Result<Void, Error>) -> Void) -> Void
) async throws {
  try await withCheckedThrowingContinuation { continuation in
    body { result in continuation.resume(with: result) }
  }
}

@available(macOS 14.0, *)
fileprivate func assertSaveRestoreSupported(
  _ configuration: VZVirtualMachineConfiguration
) throws {
  do {
    try configuration.validateSaveRestoreSupport()
    print("AppleVzRunner save/restore support: OK")
  } catch {
    print("AppleVzRunner save/restore support unavailable: \(error)")
    throw error
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

  public var canPause: Bool {
    onQueue { virtualMachine.canPause }
  }

  public var canResume: Bool {
    onQueue { virtualMachine.canResume }
  }

  public func pause(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.pause(completionHandler: completionHandler)
    }
  }

  public func resume(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.resume(completionHandler: completionHandler)
    }
  }

  @available(macOS 14.0, *)
  public func saveState(to url: URL, completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.saveMachineStateTo(url: url) { error in
        completionHandler(error.map { .failure($0) } ?? .success(()))
      }
    }
  }

  @available(macOS 14.0, *)
  public func restoreState(from url: URL, completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.restoreMachineStateFrom(url: url) { error in
        completionHandler(error.map { .failure($0) } ?? .success(()))
      }
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

// MARK: - Embedded graphical display (windowed)
//
// Hosts the running Linux VM in an on-screen `VZVirtualMachineView`. This is a
// deliberately separate path from the headless launcher above: `VZVirtualMachineView`
// requires the VM to live on the main queue, so this path creates the VM with the
// main-queue initializer and drives it from an AppKit run loop rather than the
// background-queue adapter. The headless boot + save/restore path is untouched, so a
// graphics device (which generally disables VZ save/restore) never affects it.
#if canImport(Virtualization) && canImport(AppKit)
import AppKit

/// Retains the window controller for the lifetime of the app run loop
/// (`NSApplication.delegate` does not keep a strong reference).
private var displayWindowControllerRetainer: AnyObject?

@available(macOS 14.0, *)
public extension AppleVzVirtualMachineLauncher {
  /// Boot the Linux VM with a graphics device and show it in a resizable window
  /// hosting a `VZVirtualMachineView`. Blocks in the AppKit run loop until the
  /// window is closed or the guest stops. Requires a GUI session (cannot run
  /// headless); intended to be spawned by the macOS app as a helper process.
  static func launchLinuxKernelVirtualMachineWithDisplay(
    spec: AppleVzLaunchSpec,
    options: AppleVzLaunchOptions = AppleVzLaunchOptions()
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay(
      spec: spec, sharedDirectory: options.sharedDirectorySpec)
    try configuration.validate()

    print("AppleVzRunner starting VM with display: \(spec.vmName)")
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)

    // Main-queue VM (required by VZVirtualMachineView).
    let machine = VZVirtualMachine(configuration: configuration)
    let controller = AppleVzDisplayWindowController(
      machine: machine,
      vmName: spec.vmName,
      app: app,
      stopAfterSeconds: options.stopAfterSeconds,
      forceStopGraceSeconds: options.forceStopGraceSeconds
    )
    displayWindowControllerRetainer = controller
    app.delegate = controller

    app.activate(ignoringOtherApps: true)
    app.run()
  }
}

@available(macOS 14.0, *)
final class AppleVzDisplayWindowController: NSObject, NSApplicationDelegate,
  VZVirtualMachineDelegate
{
  private let machine: VZVirtualMachine
  private let vmName: String
  private unowned let app: NSApplication
  private let stopAfterSeconds: UInt64?
  private let forceStopGraceSeconds: UInt64?
  private var window: NSWindow?
  private var signalSources: [DispatchSourceSignal] = []
  private var isStopping = false

  init(
    machine: VZVirtualMachine,
    vmName: String,
    app: NSApplication,
    stopAfterSeconds: UInt64? = nil,
    forceStopGraceSeconds: UInt64? = nil
  ) {
    self.machine = machine
    self.vmName = vmName
    self.app = app
    self.stopAfterSeconds = stopAfterSeconds
    self.forceStopGraceSeconds = forceStopGraceSeconds
    super.init()
    machine.delegate = self
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    let frame = NSRect(x: 0, y: 0, width: 1280, height: 800)
    let view = VZVirtualMachineView(frame: frame)
    view.virtualMachine = machine
    view.capturesSystemKeys = true

    let window = NSWindow(
      contentRect: frame,
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered,
      defer: false
    )
    window.title = "BridgeVM — \(vmName)"
    window.contentView = view
    window.center()
    window.makeKeyAndOrderFront(nil)
    self.window = window

    machine.start { [weak self] result in
      if case let .failure(error) = result {
        FileHandle.standardError.write(
          Data("AppleVzRunner: display VM start failed: \(error)\n".utf8))
        self?.app.terminate(nil)
      } else {
        print("AppleVzRunner display VM running: \(self?.vmName ?? "")")
      }
    }

    // Honor --stop-after-seconds (automation) by gracefully stopping then.
    if let stopAfter = stopAfterSeconds {
      DispatchQueue.main.asyncAfter(deadline: .now() + Double(stopAfter)) { [weak self] in
        self?.beginGracefulStop()
      }
    }
    // SIGTERM/SIGINT (e.g. the daemon reaper) -> request a clean guest stop
    // before AppKit terminates, instead of being torn down abruptly.
    for sig in [SIGINT, SIGTERM] {
      signal(sig, SIG_IGN)
      let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
      source.setEventHandler { [weak self] in self?.beginGracefulStop() }
      source.resume()
      signalSources.append(source)
    }
  }

  /// Ask the guest to stop, then force-stop after a grace period, then exit.
  private func beginGracefulStop() {
    guard !isStopping else { return }
    isStopping = true
    if machine.canRequestStop {
      do {
        try machine.requestStop()
      } catch {
        forceStopAndTerminate()
        return
      }
      let grace = forceStopGraceSeconds ?? 10
      DispatchQueue.main.asyncAfter(deadline: .now() + Double(grace)) { [weak self] in
        self?.forceStopAndTerminate()
      }
    } else {
      forceStopAndTerminate()
    }
  }

  private func forceStopAndTerminate() {
    if machine.canStop {
      machine.stop { [weak self] _ in self?.app.terminate(nil) }
    } else {
      app.terminate(nil)
    }
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    true
  }

  func guestDidStop(_ virtualMachine: VZVirtualMachine) {
    print("AppleVzRunner display VM guest stopped: \(vmName)")
    app.terminate(nil)
  }

  func virtualMachine(_ virtualMachine: VZVirtualMachine, didStopWithError error: Error) {
    FileHandle.standardError.write(
      Data("AppleVzRunner: display VM stopped with error: \(error)\n".utf8))
    app.terminate(nil)
  }
}
#endif

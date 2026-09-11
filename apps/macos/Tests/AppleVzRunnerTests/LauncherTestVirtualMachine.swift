import Foundation
@testable import AppleVzRunnerCore

/// Shared with launcher tasks; tests may observe state from another executor.
final class LauncherTestVirtualMachine: AppleVzVirtualMachineControlling {
  @Locked var canRequestStop = false
  @Locked var canStop = false
  @Locked var autoCompleteStart = true
  @Locked var autoNotifyStopOnForceStop = true
  @Locked private(set) var startCallCount = 0
  @Locked private(set) var requestStopCallCount = 0
  @Locked private(set) var stopCallCount = 0
  @Locked var onRequestStop: (() -> Void)?
  @Locked private var stopHandler: ((Result<Void, Error>) -> Void)?

  var hasStopHandler: Bool { stopHandler != nil }

  func start(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    startCallCount += 1
    if autoCompleteStart { completionHandler(.success(())) }
  }

  func requestStop() throws {
    requestStopCallCount += 1
    onRequestStop?()
  }

  func stop(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    stopCallCount += 1
    completionHandler(.success(()))
    if autoNotifyStopOnForceStop { stopHandler?(.success(())) }
  }

  func setStopHandler(_ handler: @escaping (Result<Void, Error>) -> Void) {
    stopHandler = handler
  }

  func completeStop(_ result: Result<Void, Error>) {
    stopHandler?(result)
  }

  // Getters release the lock before callers invoke a captured callback.
  // _modify keeps counter read/modify/write operations atomic.
  @propertyWrapper
  final class Locked<Value> {
    private let lock = NSLock()
    private var value: Value
    init(wrappedValue: Value) { value = wrappedValue }
    var wrappedValue: Value {
      get { lock.lock(); defer { lock.unlock() }; return value }
      set { lock.lock(); defer { lock.unlock() }; value = newValue }
      _modify { lock.lock(); defer { lock.unlock() }; yield &value }
    }
  }
}

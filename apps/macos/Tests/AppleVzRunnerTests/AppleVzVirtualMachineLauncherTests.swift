import Foundation
import XCTest

@testable import AppleVzRunnerCore

final class AppleVzVirtualMachineLauncherTests: XCTestCase {
  func testStartAndWaitReturnsStoppedWhenMachineStops() async throws {
    let machine = FakeVirtualMachine()
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)

    let task = Task {
      try await launcher.startAndWaitForStop(interruptionSignals: neverInterrupt())
    }

    try await waitUntil {
      machine.startCallCount == 1 && machine.hasStopHandler
    }
    machine.completeStop(.success(()))

    let result = try await task.value

    XCTAssertEqual(result, .stopped)
    XCTAssertEqual(machine.startCallCount, 1)
    XCTAssertEqual(machine.requestStopCallCount, 0)
    XCTAssertEqual(machine.stopCallCount, 0)
  }

  func testInterruptionRequestsGuestStopWhenAvailable() async throws {
    let machine = FakeVirtualMachine()
    machine.canRequestStop = true
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)

    let task = Task {
      try await launcher.startAndWaitForStop(interruptionSignals: immediateInterrupt())
    }

    try await waitUntil {
      machine.requestStopCallCount == 1
    }
    machine.completeStop(.success(()))

    let result = try await task.value

    XCTAssertEqual(result, .interrupted)
    XCTAssertEqual(machine.startCallCount, 1)
    XCTAssertEqual(machine.requestStopCallCount, 1)
    XCTAssertEqual(machine.stopCallCount, 0)
  }

  func testInterruptionFallsBackToForceStopWhenGuestStopUnavailable() async throws {
    let machine = FakeVirtualMachine()
    machine.canRequestStop = false
    machine.canStop = true
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)

    let task = Task {
      try await launcher.startAndWaitForStop(interruptionSignals: immediateInterrupt())
    }

    let result = try await task.value

    XCTAssertEqual(result, .interrupted)
    XCTAssertEqual(machine.startCallCount, 1)
    XCTAssertEqual(machine.requestStopCallCount, 0)
    XCTAssertEqual(machine.stopCallCount, 1)
  }

  func testInterruptionForceStopsAfterGraceWhenGuestStopDoesNotComplete() async throws {
    let machine = FakeVirtualMachine()
    machine.canRequestStop = true
    machine.canStop = false
    machine.onRequestStop = {
      machine.canStop = false
    }
    machine.autoNotifyStopOnForceStop = false
    let launcher = AppleVzVirtualMachineLauncher(
      machine: machine,
      clockSleep: { _ in }
    )

    let result = try await launcher.startAndWaitForStop(
      interruptionSignals: immediateInterrupt(),
      forceStopGraceSeconds: 1
    )

    XCTAssertEqual(result, .interrupted)
    XCTAssertEqual(machine.startCallCount, 1)
    XCTAssertEqual(machine.requestStopCallCount, 1)
    XCTAssertEqual(machine.stopCallCount, 1)
  }

  func testInterruptionCanStopMachineBeforeStartCompletionReturns() async throws {
    let machine = FakeVirtualMachine()
    machine.autoCompleteStart = false
    machine.canRequestStop = true
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)

    let task = Task {
      try await launcher.startAndWaitForStop(interruptionSignals: immediateInterrupt())
    }

    try await waitUntil {
      machine.requestStopCallCount == 1
    }
    machine.completeStop(.success(()))

    let result = try await task.value

    XCTAssertEqual(result, .interrupted)
    XCTAssertEqual(machine.startCallCount, 1)
    XCTAssertEqual(machine.requestStopCallCount, 1)
    XCTAssertEqual(machine.stopCallCount, 0)
  }

  private func neverInterrupt() -> AsyncStream<Void> {
    AsyncStream { _ in }
  }

  private func immediateInterrupt() -> AsyncStream<Void> {
    AsyncStream { continuation in
      continuation.yield()
      continuation.finish()
    }
  }

  private func waitUntil(
    timeout: Duration = .seconds(1),
    condition: @escaping () -> Bool
  ) async throws {
    let deadline = ContinuousClock.now + timeout
    while !condition() {
      if ContinuousClock.now >= deadline {
        XCTFail("Timed out waiting for condition")
        return
      }
      try await Task.sleep(for: .milliseconds(10))
    }
  }
}

private final class FakeVirtualMachine: AppleVzVirtualMachineControlling {
  var canRequestStop = false
  var canStop = false
  var autoCompleteStart = true
  var autoNotifyStopOnForceStop = true
  private(set) var startCallCount = 0
  private(set) var requestStopCallCount = 0
  private(set) var stopCallCount = 0
  var onRequestStop: (() -> Void)?
  private var stopHandler: ((Result<Void, Error>) -> Void)?

  var hasStopHandler: Bool {
    stopHandler != nil
  }

  func start(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    startCallCount += 1
    if autoCompleteStart {
      completionHandler(.success(()))
    }
  }

  func requestStop() throws {
    requestStopCallCount += 1
    onRequestStop?()
  }

  func stop(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    stopCallCount += 1
    completionHandler(.success(()))
    if autoNotifyStopOnForceStop {
      stopHandler?(.success(()))
    }
  }

  func setStopHandler(_ handler: @escaping (Result<Void, Error>) -> Void) {
    stopHandler = handler
  }

  func completeStop(_ result: Result<Void, Error>) {
    stopHandler?(result)
  }
}

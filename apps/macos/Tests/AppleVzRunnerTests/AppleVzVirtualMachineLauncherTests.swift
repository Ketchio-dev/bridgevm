import Foundation
import XCTest

@testable import AppleVzRunnerCore

final class AppleVzVirtualMachineLauncherTests: XCTestCase {
  func testStartAndWaitReturnsStoppedWhenMachineStops() async throws {
    let machine = LauncherTestVirtualMachine()
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
    let machine = LauncherTestVirtualMachine()
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
    let machine = LauncherTestVirtualMachine()
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
    let machine = LauncherTestVirtualMachine()
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
    let machine = LauncherTestVirtualMachine()
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

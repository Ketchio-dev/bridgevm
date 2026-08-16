import XCTest

@testable import BridgeVMApp

/// The spawn settle window is paid on the main actor, so its size is a UI
/// property rather than an implementation detail. Split out of
/// EmbeddedDisplayLauncherTests.swift, which is at its ceiling.
final class EmbeddedDisplaySpawnWindowTests: XCTestCase {
  func testSpawnSettleWindowStaysSmallEnoughToNotStallTheUI() {
    // runDetached blocks the caller, and the caller is the main actor, so this
    // window is what opening a display costs the UI. It was 100 ms, chosen
    // without a measurement; a helper that fails at startup is observable in
    // under 2 ms, and a 20 ms window caught it 60 times out of 60 with no
    // healthy helper misreported.
    XCTAssertTrue(
      EmbeddedDisplayLauncher.spawnSettleWindow <= 0.02,
      "this window is paid on the main actor every time a display opens")
    XCTAssertTrue(
      EmbeddedDisplayLauncher.spawnSettleWindow > 0.005,
      "too short and a helper that fails at startup is reported as healthy")
  }
}

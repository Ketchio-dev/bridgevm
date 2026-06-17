import XCTest

@testable import BridgeVMApp

final class EmbeddedDisplayLauncherTests: XCTestCase {
  func testRunnerArgumentsBuildVmNameDisplayLaunch() {
    let args = EmbeddedDisplayLauncher.runnerArguments(
      vmName: "ubuntu-dev",
      appleVzRunnerPath: "/Helpers/AppleVzRunner"
    )
    XCTAssertEqual(
      args,
      [
        "ubuntu-dev",
        "--launch",
        "--require-ready",
        "--apple-vz-runner",
        "/Helpers/AppleVzRunner",
        "--apple-vz-allow-real-start",
        "--apple-vz-display",
      ]
    )
    // No --store override: the runner uses the same default store as the daemon.
    XCTAssertFalse(args.contains("--store"))
  }

  func testLaunchResolvesHelpersAndSpawnsRunner() throws {
    let lightvm = URL(fileURLWithPath: "/Helpers/lightvm-runner")
    let appleVz = URL(fileURLWithPath: "/Helpers/AppleVzRunner")
    var spawnedExecutable: URL?
    var spawnedArgs: [String] = []

    _ = try EmbeddedDisplayLauncher.launch(
      vmName: "win-or-linux",
      helperResolver: { name in
        switch name {
        case "lightvm-runner": return lightvm
        case "AppleVzRunner": return appleVz
        default: return nil
        }
      },
      spawn: { url, args in
        spawnedExecutable = url
        spawnedArgs = args
        return Process()
      }
    )

    XCTAssertEqual(spawnedExecutable, lightvm)
    XCTAssertEqual(spawnedArgs.first, "win-or-linux")
    XCTAssertTrue(spawnedArgs.contains("--apple-vz-display"))
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-runner")! + 1],
      appleVz.path
    )
  }

  func testLaunchThrowsWhenHelperMissing() {
    XCTAssertThrowsError(
      try EmbeddedDisplayLauncher.launch(
        vmName: "x",
        helperResolver: { _ in nil },
        spawn: { _, _ in Process() }
      )
    ) { error in
      XCTAssertEqual(
        error as? EmbeddedDisplayLauncher.LaunchError,
        .helperMissing("lightvm-runner")
      )
    }
  }
}

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
        "--apple-vz-display-width",
        "1280",
        "--apple-vz-display-height",
        "800",
      ]
    )
    // No --store override: the runner uses the same default store as the daemon.
    XCTAssertFalse(args.contains("--store"))
  }

  func testRunnerArgumentsAcceptStorePath() throws {
    let args = EmbeddedDisplayLauncher.runnerArguments(
      vmName: "ubuntu-dev",
      appleVzRunnerPath: "/Helpers/AppleVzRunner",
      storePath: "/Volumes/BridgeVM Store"
    )

    XCTAssertTrue(containsPair(args, "--store", "/Volumes/BridgeVM Store"))
    XCTAssertLessThan(
      try XCTUnwrap(args.firstIndex(of: "--store")),
      try XCTUnwrap(args.firstIndex(of: "--launch"))
    )
  }

  func testRunnerArgumentsAcceptCustomDisplaySize() {
    let args = EmbeddedDisplayLauncher.runnerArguments(
      vmName: "ubuntu-dev",
      appleVzRunnerPath: "/Helpers/AppleVzRunner",
      displaySize: .init(width: 1440, height: 900)
    )

    XCTAssertEqual(
      Array(args.suffix(4)),
      ["--apple-vz-display-width", "1440", "--apple-vz-display-height", "900"]
    )
  }

  func testRunnerArgumentsAcceptRuntimeControlSocket() {
    let args = EmbeddedDisplayLauncher.runnerArguments(
      vmName: "ubuntu-dev",
      appleVzRunnerPath: "/Helpers/AppleVzRunner",
      runtimeControlSocketPath: "/tmp/ubuntu-dev.sock"
    )

    XCTAssertEqual(
      Array(args.suffix(2)),
      ["--apple-vz-runtime-control-socket", "/tmp/ubuntu-dev.sock"]
    )
  }

  func testRunnerArgumentsAcceptProxyFramebufferExport() {
    let args = EmbeddedDisplayLauncher.runnerArguments(
      vmName: "ubuntu-dev",
      appleVzRunnerPath: "/Helpers/AppleVzRunner",
      proxyFramebufferRGBAPath: "/tmp/ubuntu-dev.rgba",
      proxyFramebufferCaptureIntervalMillis: 250
    )

    XCTAssertTrue(args.contains("--apple-vz-display"))
    XCTAssertTrue(
      containsPair(args, "--apple-vz-proxy-framebuffer-rgba-file", "/tmp/ubuntu-dev.rgba")
    )
    XCTAssertTrue(
      containsPair(args, "--apple-vz-proxy-framebuffer-capture-interval-ms", "250")
    )
  }

  func testDefaultRuntimeControlSocketPathUsesBridgeVmHomeAndStoreSlug() {
    XCTAssertEqual(
      EmbeddedDisplayLauncher.defaultRuntimeControlSocketPath(
        vmName: "Ubuntu Dev",
        environment: ["BRIDGEVM_HOME": "/tmp/bridgevm-home", "HOME": "/Users/example"]
      ),
      "/tmp/bvm-vz-697163b713e342a7.sock"
    )
  }

  func testDefaultRuntimeControlSocketPathFallsBackToHomeDotBridgeVm() {
    XCTAssertEqual(
      EmbeddedDisplayLauncher.defaultRuntimeControlSocketPath(
        vmName: "Ubuntu Dev",
        environment: ["HOME": "/Users/example"]
      ),
      "/tmp/bvm-vz-df46a17c9c76e10d.sock"
    )
  }

  func testDefaultProxyFramebufferRGBAPathUsesBridgeVmHomeAndStoreSlug() {
    XCTAssertEqual(
      EmbeddedDisplayLauncher.defaultProxyFramebufferRGBAPath(
        vmName: "Ubuntu Dev",
        environment: ["BRIDGEVM_HOME": "/tmp/bridgevm-home", "HOME": "/Users/example"]
      ),
      "/tmp/bridgevm-home/vms/ubuntu-dev.vmbridge/metadata/apple-vz-display-framebuffer.rgba"
    )
  }

  func testStoreMetadataDerivesDisplayPathsFromCustomStoreRoot() {
    let metadata = EmbeddedDisplayLauncher.StoreMetadata(
      storeRoot: "/Volumes/BridgeVM Store"
    )

    XCTAssertEqual(
      EmbeddedDisplayLauncher.effectiveStorePath(storeMetadata: metadata),
      "/Volumes/BridgeVM Store"
    )
    XCTAssertEqual(
      EmbeddedDisplayLauncher.effectiveBundlePath(
        vmName: "Ubuntu Dev",
        storeMetadata: metadata,
        environment: ["BRIDGEVM_HOME": "/tmp/ignored"]
      ),
      "/Volumes/BridgeVM Store/vms/ubuntu-dev.vmbridge"
    )
    XCTAssertEqual(
      EmbeddedDisplayLauncher.proxyFramebufferRGBAPath(
        vmName: "Ubuntu Dev",
        storeMetadata: metadata,
        environment: ["BRIDGEVM_HOME": "/tmp/ignored"]
      ),
      "/Volumes/BridgeVM Store/vms/ubuntu-dev.vmbridge/metadata/apple-vz-display-framebuffer.rgba"
    )
    XCTAssertNotEqual(
      EmbeddedDisplayLauncher.runtimeControlSocketPath(
        vmName: "Ubuntu Dev",
        storeMetadata: metadata,
        environment: ["BRIDGEVM_HOME": "/tmp/ignored"]
      ),
      EmbeddedDisplayLauncher.defaultRuntimeControlSocketPath(
        vmName: "Ubuntu Dev",
        environment: ["BRIDGEVM_HOME": "/tmp/ignored"]
      )
    )
  }

  func testStoreMetadataDerivesStoreAndDisplayPathsFromActualBundlePath() {
    let metadata = EmbeddedDisplayLauncher.StoreMetadata(
      bundlePath: "/Volumes/BridgeVM Store/vms/dev-from-daemon.vmbridge"
    )

    XCTAssertEqual(
      EmbeddedDisplayLauncher.effectiveStorePath(storeMetadata: metadata),
      "/Volumes/BridgeVM Store"
    )
    XCTAssertEqual(
      EmbeddedDisplayLauncher.effectiveBundlePath(
        vmName: "Ubuntu Dev",
        storeMetadata: metadata,
        environment: ["BRIDGEVM_HOME": "/tmp/ignored"]
      ),
      "/Volumes/BridgeVM Store/vms/dev-from-daemon.vmbridge"
    )
    XCTAssertEqual(
      EmbeddedDisplayLauncher.proxyFramebufferRGBAPath(
        vmName: "Ubuntu Dev",
        storeMetadata: metadata,
        environment: ["BRIDGEVM_HOME": "/tmp/ignored"]
      ),
      "/Volumes/BridgeVM Store/vms/dev-from-daemon.vmbridge/metadata/apple-vz-display-framebuffer.rgba"
    )
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
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-display-width")! + 1],
      "1280"
    )
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-display-height")! + 1],
      "800"
    )
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-runtime-control-socket")! + 1],
      EmbeddedDisplayLauncher.defaultRuntimeControlSocketPath(vmName: "win-or-linux")
    )
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-proxy-framebuffer-rgba-file")! + 1],
      EmbeddedDisplayLauncher.defaultProxyFramebufferRGBAPath(vmName: "win-or-linux")
    )
    XCTAssertFalse(spawnedArgs.contains("--store"))
  }

  func testLaunchPassesCustomStoreAndBundleDerivedDisplayPaths() throws {
    let lightvm = URL(fileURLWithPath: "/Helpers/lightvm-runner")
    let appleVz = URL(fileURLWithPath: "/Helpers/AppleVzRunner")
    let metadata = EmbeddedDisplayLauncher.StoreMetadata(
      bundlePath: "/Volumes/BridgeVM Store/vms/custom-dev.vmbridge"
    )
    var spawnedArgs: [String] = []

    _ = try EmbeddedDisplayLauncher.launch(
      vmName: "Dev VM",
      storeMetadata: metadata,
      helperResolver: { name in
        switch name {
        case "lightvm-runner": return lightvm
        case "AppleVzRunner": return appleVz
        default: return nil
        }
      },
      spawn: { _, args in
        spawnedArgs = args
        return Process()
      }
    )

    XCTAssertTrue(containsPair(spawnedArgs, "--store", "/Volumes/BridgeVM Store"))
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-runtime-control-socket")! + 1],
      EmbeddedDisplayLauncher.runtimeControlSocketPath(
        vmName: "Dev VM",
        storeMetadata: metadata
      )
    )
    XCTAssertEqual(
      spawnedArgs[spawnedArgs.firstIndex(of: "--apple-vz-proxy-framebuffer-rgba-file")! + 1],
      "/Volumes/BridgeVM Store/vms/custom-dev.vmbridge/metadata/apple-vz-display-framebuffer.rgba"
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

  private func containsPair(_ args: [String], _ first: String, _ second: String) -> Bool {
    args.indices.dropLast().contains { index in
      args[index] == first && args[index + 1] == second
    }
  }
}

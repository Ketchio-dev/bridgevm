#if canImport(Darwin)
import Darwin
import Foundation
import XCTest

#if canImport(AppKit)
import AppKit
#endif

@testable import AppleVzRunnerCore

final class AppleVzDisplayRuntimeControlServerTests: XCTestCase {
  func testStatusAndStopCommandsReturnSnapshot() throws {
    let socketPath = makeShortSocketPath()
    var stopCount = 0
    var state = "running"

    let server = AppleVzDisplayRuntimeControlServer(
      socketPath: socketPath,
      statusProvider: {
        AppleVzDisplayRuntimeControlSnapshot(
          vmName: "ubuntu-dev",
          state: state,
          displayWidthInPixels: 1280,
          displayHeightInPixels: 800,
          isStopping: stopCount > 0
        )
      },
      stopHandler: {
        stopCount += 1
        state = "stopping"
      }
    )
    try server.start()
    defer {
      server.stop()
      try? FileManager.default.removeItem(atPath: socketPath)
    }

    let status = try sendRuntimeControlCommand("status", to: socketPath)
    XCTAssertEqual(status["ok"] as? Bool, true)
    XCTAssertEqual(status["vm"] as? String, "ubuntu-dev")
    XCTAssertEqual(status["state"] as? String, "running")
    XCTAssertEqual(status["stopping"] as? Bool, false)
    XCTAssertEqual(status["supported_commands"] as? [String], ["status", "stop", "policy", "pacing"])
    let runtimePolicy = try XCTUnwrap(status["runtime_policy"] as? [String: Any])
    XCTAssertEqual(runtimePolicy["available"] as? Bool, false)
    let display = try XCTUnwrap(status["display"] as? [String: Any])
    XCTAssertEqual(display["width"] as? Int, 1280)
    XCTAssertEqual(display["height"] as? Int, 800)
    let framebufferExport = try XCTUnwrap(status["framebuffer_export"] as? [String: Any])
    XCTAssertEqual(framebufferExport["enabled"] as? Bool, false)

    let stop = try sendRuntimeControlCommand("stop", to: socketPath)
    XCTAssertEqual(stop["ok"] as? Bool, true)
    XCTAssertEqual(stop["accepted"] as? Bool, true)
    XCTAssertEqual(stop["state"] as? String, "stopping")
    XCTAssertEqual(stop["stopping"] as? Bool, true)
    XCTAssertEqual(stopCount, 1)
  }

  func testPolicyCommandReturnsLatestRuntimePolicy() throws {
    let socketPath = makeShortSocketPath()
    var policy: [String: Any] = [
      "vm": "ubuntu-dev",
      "visibility": "foreground",
      "memory": "4096",
      "cpu": "2",
      "display_fps_cap": "adaptive",
    ]

    let server = AppleVzDisplayRuntimeControlServer(
      socketPath: socketPath,
      statusProvider: {
        AppleVzDisplayRuntimeControlSnapshot(
          vmName: "ubuntu-dev",
          state: "running",
          displayWidthInPixels: 1280,
          displayHeightInPixels: 800,
          isStopping: false
        )
      },
      stopHandler: {},
      runtimePolicyProvider: { policy }
    )
    try server.start()
    defer {
      server.stop()
      try? FileManager.default.removeItem(atPath: socketPath)
    }

    let first = try sendRuntimeControlCommand("policy", to: socketPath)
    XCTAssertEqual(first["ok"] as? Bool, true)
    XCTAssertEqual(first["supported_commands"] as? [String], ["status", "stop", "policy", "pacing"])
    let firstPolicy = try XCTUnwrap(first["policy"] as? [String: Any])
    XCTAssertEqual(firstPolicy["visibility"] as? String, "foreground")
    XCTAssertEqual(firstPolicy["memory"] as? String, "4096")

    policy["visibility"] = "background"
    policy["memory"] = "2048"

    let second = try sendRuntimeControlCommand("policy", to: socketPath)
    XCTAssertEqual(second["ok"] as? Bool, true)
    let secondPolicy = try XCTUnwrap(second["policy"] as? [String: Any])
    XCTAssertEqual(secondPolicy["visibility"] as? String, "background")
    XCTAssertEqual(secondPolicy["memory"] as? String, "2048")
  }

  func testPacingCommandSummarizesRuntimePolicyDisplayCap() throws {
    let socketPath = makeShortSocketPath()
    var policy: [String: Any] = [
      "vm": "ubuntu-dev",
      "visibility": "background",
      "memory": "2048",
      "cpu": "1",
      "display_fps_cap": "10",
    ]

    let server = AppleVzDisplayRuntimeControlServer(
      socketPath: socketPath,
      statusProvider: {
        AppleVzDisplayRuntimeControlSnapshot(
          vmName: "ubuntu-dev",
          state: "running",
          displayWidthInPixels: 1280,
          displayHeightInPixels: 800,
          isStopping: false
        )
      },
      stopHandler: {},
      runtimePolicyProvider: { policy }
    )
    try server.start()
    defer {
      server.stop()
      try? FileManager.default.removeItem(atPath: socketPath)
    }

    let capped = try sendRuntimeControlCommand("pacing", to: socketPath)
    XCTAssertEqual(capped["ok"] as? Bool, true)
    XCTAssertEqual(capped["visibility"] as? String, "background")
    XCTAssertEqual(capped["display_fps_cap"] as? String, "10")
    XCTAssertEqual(capped["max_fps"] as? Int, 10)
    XCTAssertEqual(capped["supported_commands"] as? [String], ["status", "stop", "policy", "pacing"])

    policy["visibility"] = "foreground"
    policy["display_fps_cap"] = "adaptive"

    let adaptive = try sendRuntimeControlCommand("pacing", to: socketPath)
    XCTAssertEqual(adaptive["ok"] as? Bool, true)
    XCTAssertEqual(adaptive["visibility"] as? String, "foreground")
    XCTAssertEqual(adaptive["display_fps_cap"] as? String, "adaptive")
    XCTAssertEqual(adaptive["max_fps"] as? String, "adaptive")
  }

  func testUnknownCommandReportsSupportedCommands() throws {
    let socketPath = makeShortSocketPath()
    let server = AppleVzDisplayRuntimeControlServer(
      socketPath: socketPath,
      statusProvider: {
        AppleVzDisplayRuntimeControlSnapshot(
          vmName: "ubuntu-dev",
          state: "running",
          displayWidthInPixels: 1024,
          displayHeightInPixels: 768,
          isStopping: false
        )
      },
      stopHandler: {}
    )
    try server.start()
    defer {
      server.stop()
      try? FileManager.default.removeItem(atPath: socketPath)
    }

    let response = try sendRuntimeControlCommand("bogus", to: socketPath)
    XCTAssertEqual(response["ok"] as? Bool, false)
    XCTAssertEqual(response["error"] as? String, "unknown-command")
    XCTAssertEqual(response["supported_commands"] as? [String], ["status", "stop", "policy", "pacing"])
  }

  func testStatusReportsFramebufferExportPathWhenConfigured() throws {
    let socketPath = makeShortSocketPath()
    let framebufferPath = "/tmp/ubuntu-dev-display.rgba"
    let server = AppleVzDisplayRuntimeControlServer(
      socketPath: socketPath,
      statusProvider: {
        AppleVzDisplayRuntimeControlSnapshot(
          vmName: "ubuntu-dev",
          state: "running",
          displayWidthInPixels: 1024,
          displayHeightInPixels: 768,
          isStopping: false,
          proxyFramebufferRGBAPath: framebufferPath,
          proxyFramebufferCaptureIntervalMillis: 250
        )
      },
      stopHandler: {}
    )
    try server.start()
    defer {
      server.stop()
      try? FileManager.default.removeItem(atPath: socketPath)
    }

    let response = try sendRuntimeControlCommand("status", to: socketPath)
    let framebufferExport = try XCTUnwrap(response["framebuffer_export"] as? [String: Any])
    XCTAssertEqual(framebufferExport["enabled"] as? Bool, true)
    XCTAssertEqual(framebufferExport["path"] as? String, framebufferPath)
    XCTAssertEqual(framebufferExport["interval_millis"] as? Int, 250)
  }

  #if canImport(AppKit) && canImport(Virtualization)
  func testFramebufferExporterConvertsCGImageToRGBABytes() throws {
    guard #available(macOS 14.0, *) else {
      throw XCTSkip("framebuffer exporter is only compiled for the macOS 14 VZ display path")
    }
    let bytes = Data([0x11, 0x22, 0x33, 0xFF])
    let provider = try XCTUnwrap(CGDataProvider(data: bytes as CFData))
    let image = try XCTUnwrap(
      CGImage(
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bitsPerPixel: 32,
        bytesPerRow: 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
        provider: provider,
        decode: nil,
        shouldInterpolate: false,
        intent: .defaultIntent
      )
    )

    let rgba = try AppleVzDisplayFramebufferExporter.rgbaData(from: image, width: 1, height: 1)

    XCTAssertEqual(Array(rgba), [0x11, 0x22, 0x33, 0xFF])
  }
  #endif

  private func sendRuntimeControlCommand(
    _ command: String,
    to socketPath: String
  ) throws -> [String: Any] {
    let request = try JSONSerialization.data(
      withJSONObject: ["command": command],
      options: []
    )
    let response = try withConnectedSocket(to: socketPath) { fd in
      try request.withUnsafeBytes { rawBuffer in
        guard let baseAddress = rawBuffer.baseAddress else {
          return
        }
        let written = write(fd, baseAddress, request.count)
        guard written == request.count else {
          throw RuntimeControlClientError.writeFailed
        }
      }

      var buffer = [UInt8](repeating: 0, count: 4096)
      let count = read(fd, &buffer, buffer.count)
      guard count > 0 else {
        throw RuntimeControlClientError.readFailed
      }
      return Data(buffer.prefix(count))
    }

    let object = try JSONSerialization.jsonObject(with: response)
    guard let dictionary = object as? [String: Any] else {
      throw RuntimeControlClientError.invalidJSON
    }
    return dictionary
  }

  private func makeShortSocketPath() -> String {
    let suffix = UUID().uuidString.prefix(8)
    return "/tmp/bvm-rc-\(getpid())-\(suffix).sock"
  }

  private func withConnectedSocket<T>(
    to socketPath: String,
    _ body: (Int32) throws -> T
  ) throws -> T {
    var lastErrno: Int32 = 0
    for _ in 0..<100 {
      let fd = socket(AF_UNIX, SOCK_STREAM, 0)
      guard fd >= 0 else {
        throw RuntimeControlClientError.socketFailed(errno)
      }

      if connectSocket(fd, to: socketPath) {
        defer {
          close(fd)
        }
        return try body(fd)
      }

      lastErrno = errno
      close(fd)
      usleep(10_000)
    }
    throw RuntimeControlClientError.connectFailed(lastErrno)
  }

  private func connectSocket(_ fd: Int32, to socketPath: String) -> Bool {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let pathBytes = Array(socketPath.utf8)
    let sunPathCapacity = MemoryLayout.size(ofValue: address.sun_path)
    guard pathBytes.count < sunPathCapacity else {
      return false
    }

    withUnsafeMutablePointer(to: &address.sun_path) { pointer in
      pointer.withMemoryRebound(to: CChar.self, capacity: sunPathCapacity) { sunPath in
        for index in 0..<sunPathCapacity {
          sunPath[index] = 0
        }
        for (index, byte) in pathBytes.enumerated() {
          sunPath[index] = CChar(bitPattern: byte)
        }
      }
    }

    let result = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
        connect(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    return result == 0
  }
}

private enum RuntimeControlClientError: Error {
  case socketFailed(Int32)
  case connectFailed(Int32)
  case writeFailed
  case readFailed
  case invalidJSON
}
#endif

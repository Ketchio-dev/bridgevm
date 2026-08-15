import Foundation
import XCTest

@testable import BridgeVMApp

// The daemon DTOs the app decodes by hand, and the transport that reaches them.
// Split out of DaemonDTOTests.swift, which is already 5,000 lines of one class.
final class DaemonDecoderTests: XCTestCase {
  func testResourcesDecodePreferCanonicalKeysAndConvertUnits() throws {
    func decode(_ json: String) throws -> DaemonVirtualMachineResourcesDTO {
      try JSONDecoder().decode(
        DaemonVirtualMachineResourcesDTO.self, from: Data(json.utf8))
    }

    let camel = try decode(#"{"cpuCount":8,"memoryGB":16,"diskGB":100}"#)
    XCTAssertEqual(camel.cpuCount, 8)
    XCTAssertEqual(camel.memoryGB, 16)
    XCTAssertEqual(camel.diskGB, 100)

    let snake = try decode(#"{"cpu_count":4,"memory_gb":8,"disk_gb":50}"#)
    XCTAssertEqual(snake.cpuCount, 4)
    XCTAssertEqual(snake.memoryGB, 8)
    XCTAssertEqual(snake.diskGB, 50)

    let vcpus = try decode(#"{"vcpus":2}"#)
    XCTAssertEqual(vcpus.cpuCount, 2)

    // Byte and mebibyte forms are converted, and a canonical key still wins
    // over them when both are present.
    let derived = try decode(#"{"memory_mib":4096,"disk_bytes":107374182400}"#)
    XCTAssertEqual(derived.memoryGB, 4)
    XCTAssertEqual(derived.diskGB, 100)
    let bytes = try decode(#"{"memory_bytes":8589934592}"#)
    XCTAssertEqual(bytes.memoryGB, 8)
    let preferred = try decode(#"{"memory_gb":16,"memory_mib":1024}"#)
    XCTAssertEqual(preferred.memoryGB, 16)

    let empty = try decode("{}")
    XCTAssertNil(empty.cpuCount)
    XCTAssertNil(empty.memoryGB)
    XCTAssertNil(empty.diskGB)
  }

  func testBootMediaKindAcceptsEverySpellingTheDaemonSends() throws {
    func decode(_ kind: String) throws -> DaemonBootMediaStatusEntryDTO {
      let json = "{\"kind\":\"\(kind)\",\"path\":\"/tmp/x\",\"exists\":true}"
      return try JSONDecoder().decode(
        DaemonBootMediaStatusEntryDTO.self, from: Data(json.utf8))
    }

    // BootMediaKind is #[serde(rename_all = "kebab-case")] in records.rs, so
    // these four are the exact strings the daemon puts on the wire. An unmapped
    // one decodes to .unknown with no error, which is why each is pinned.
    XCTAssertEqual(try decode("installer-image").kind, .installerImage)
    XCTAssertEqual(try decode("kernel").kind, .kernel)
    XCTAssertEqual(try decode("initrd").kind, .initrd)
    XCTAssertEqual(try decode("macos-restore-image").kind, .macosRestoreImage)
    XCTAssertEqual(try decode("something-else").kind, .unknown)
  }

  func testBootMediaSizeFallsBackToTheDaemonBytesKey() throws {
    func decode(_ json: String) throws -> DaemonBootMediaStatusEntryDTO {
      try JSONDecoder().decode(DaemonBootMediaStatusEntryDTO.self, from: Data(json.utf8))
    }

    // The Rust record's field is `bytes`; `size_bytes` is the preferred alias.
    let alias = try decode("{\"kind\":\"kernel\",\"path\":\"/k\",\"exists\":true,\"size_bytes\":10}")
    XCTAssertEqual(alias.sizeBytes, 10)
    let wire = try decode("{\"kind\":\"kernel\",\"path\":\"/k\",\"exists\":true,\"bytes\":20}")
    XCTAssertEqual(wire.sizeBytes, 20)
    let both = try decode(
      "{\"kind\":\"kernel\",\"path\":\"/k\",\"exists\":true,\"size_bytes\":10,\"bytes\":20}")
    XCTAssertEqual(both.sizeBytes, 10)
    let neither = try decode("{\"kind\":\"kernel\",\"path\":\"/k\",\"exists\":true}")
    XCTAssertNil(neither.sizeBytes)
  }

  func testLiveEvidenceProofFlagsDefaultToUnproven() throws {
    // Each of these is #[serde(default)] on the Rust side, so an older daemon
    // omits them. Defaulting them to true would claim evidence that does not
    // exist, so the direction of the default is the thing worth pinning.
    let json = """
      {"path":"/p","backend":"hvf","vm_name":"win","boot_mode":"uefi",
       "disk_format":"raw","network":"nat","serial_sentinel_required":true,
       "serial_sentinel_proven":true,"summary":"ok"}
      """
    let dto = try JSONDecoder().decode(DaemonLiveEvidenceDTO.self, from: Data(json.utf8))
    XCTAssertTrue(dto.serialSentinelProven)
    XCTAssertFalse(dto.graphicalBootProgressProven)
    XCTAssertFalse(dto.viewerEvidenceProven)
    XCTAssertFalse(dto.qmpEvidenceProven)
    XCTAssertFalse(dto.guestToolsEffectsProven)
  }

  func testMissingDaemonSocketFailsWithoutWaitingForTheRequestTimeout() async throws {
    // A Unix socket that does not exist puts NWConnection into .waiting rather
    // than .failed, and it stays there: a connection to a missing path was
    // measured still waiting after four seconds, and creating the socket later
    // did not make it ready. Treating .waiting as a failure is what makes the
    // app report "daemon not running" immediately instead of after the request
    // timeout, which is two seconds for a quick request and ten minutes for an
    // archive one.
    let path = "/tmp/bridgevm-tests/missing-\(UUID().uuidString)/bridgevmd.sock"
    let transport = UnixSocketNDJSONTransport(endpoint: DaemonEndpoint(socketPath: path))

    let start = Date()
    do {
      _ = try await transport.send(["type": "state"], responseType: DaemonStateResponse.self)
      XCTFail("connecting to a missing socket must not succeed")
    } catch {
      // Any error is correct here; the point is that it arrives promptly.
    }
    let elapsed = Date().timeIntervalSince(start)

    // The quick-request timeout is 2 s. Anything at or above that means the
    // connection sat in .waiting until the timeout fired. One second leaves
    // ample room for a loaded machine while still failing the defect, which
    // took the full 2 s.
    XCTAssertLessThan(elapsed, 1.0, "took \(elapsed)s; the .waiting state is not being failed")
  }

}

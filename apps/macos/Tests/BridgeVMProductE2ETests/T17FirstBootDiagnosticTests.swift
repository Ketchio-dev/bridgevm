import CryptoKit
import XCTest
@testable import BridgeVMProductE2E

final class T17FirstBootDiagnosticTests: XCTestCase {
    func testMissingAndUnsafeInputsStayDistinct() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let missing = root.appendingPathComponent("missing.log")
        XCTAssertEqual(T17FirstBootDiagnostic.capture(missing), "run_log=status=absent")
        let target = root.appendingPathComponent("target.log")
        try Data("guest".utf8).write(to: target)
        let link = root.appendingPathComponent("link.log")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        XCTAssertEqual(T17FirstBootDiagnostic.capture(link), "run_log=status=unsafe")
    }

    func testEmptyLogHasAnAuthenticatedEmptyCapture() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: Data()))
        defer { try? FileManager.default.removeItem(at: url) }
        let emptyHash = SHA256.hash(data: Data()).map { String(format: "%02x", $0) }.joined()
        XCTAssertEqual(T17FirstBootDiagnostic.capture(url),
            "run_log=status=empty,bytes=0,captured=0,truncated=0,sha256=\(emptyHash),ready=0,service_start=0,system_reset=0,system_off=0,console_stats=0")
    }

    func testOnlyAllowListedMarkerCountsEnterTheSummary() {
        let data = Data("private guest output\nBVAGENT SERVICE start\nBVAGENT READY host=secret\nPSCI SYSTEM_RESET\nstop: PSCI SYSTEM_OFF\nvirtio-console stats rx=1\n".utf8)
        let detail = T17FirstBootDiagnostic.summarize(data, totalBytes: UInt64(data.count))
        XCTAssertFalse(detail.contains("private guest output"))
        XCTAssertFalse(detail.contains("host=secret"))
        XCTAssertTrue(detail.hasSuffix("ready=1,service_start=1,system_reset=1,system_off=1,console_stats=1"))
    }

    func testCaptureHashesOnlyTheBoundedTail() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("prefix-BVAGENT READY\ntail-virtio-console stats\n".utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let detail = T17FirstBootDiagnostic.capture(url, maxBytes: 26)
        XCTAssertTrue(detail.contains("captured=26,truncated=1"))
        XCTAssertTrue(detail.hasSuffix("ready=0,service_start=0,system_reset=0,system_off=0,console_stats=1"))
    }
}

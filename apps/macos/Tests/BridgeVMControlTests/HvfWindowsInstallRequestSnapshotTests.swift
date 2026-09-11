import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallRequestSnapshotTests: XCTestCase {
    func testCapturedBytesAndDigestSurvivePathReplacement() throws {
        try fixture { url in
            let request = makeRequest("/synthetic/first.iso")
            let bytes = try JSONEncoder().encode(request)
            try bytes.write(to: url)
            let expected = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(url.path))
            let snapshot = try HvfWindowsInstallRequestSnapshot.load(url, expectedSHA256: expected)
            try JSONEncoder().encode(makeRequest("/synthetic/second.iso")).write(to: url, options: .atomic)
            XCTAssertEqual(snapshot.request, request)
            XCTAssertEqual(snapshot.data, bytes)
            XCTAssertEqual(snapshot.sha256, expected)
            XCTAssertThrowsError(try HvfWindowsInstallRequestSnapshot.load(url, expectedSHA256: expected))
            XCTAssertThrowsError(try HvfWindowsInstallFinalization.validateRequest(url, expectedSHA256: expected))
        }
    }

    func testEquivalentJSONStillRequiresExactByteDigest() throws {
        let bytes = try JSONEncoder().encode(makeRequest("/synthetic/windows.iso"))
        let snapshot = try HvfWindowsInstallRequestSnapshot(data: bytes)
        var padded = Data("\n".utf8)
        padded.append(bytes)
        let alternate = try HvfWindowsInstallRequestSnapshot(data: padded)
        XCTAssertEqual(snapshot.request, alternate.request)
        XCTAssertNotEqual(snapshot.sha256, alternate.sha256)
        XCTAssertThrowsError(try HvfWindowsInstallRequestSnapshot(data: padded, expectedSHA256: snapshot.sha256))
    }

    func testMalformedAndOversizedRequestsAreRejected() {
        XCTAssertThrowsError(try HvfWindowsInstallRequestSnapshot(data: Data("{}".utf8)))
        XCTAssertThrowsError(try HvfWindowsInstallRequestSnapshot(
            data: Data(repeating: 0x20, count: VMLibrary.maximumConfigBytes + 1)))
    }

    func testExistingValidatorAcceptsMatchingCapturedRequest() throws {
        try fixture { url in
            let bytes = try JSONEncoder().encode(makeRequest("/synthetic/windows.iso"))
            try bytes.write(to: url)
            let snapshot = try HvfWindowsInstallRequestSnapshot.load(url)
            XCTAssertNoThrow(try HvfWindowsInstallFinalization.validateRequest(url, expectedSHA256: snapshot.sha256))
            XCTAssertNoThrow(try HvfWindowsInstallFinalization.validateRequest(url))
        }
    }

    private func makeRequest(_ iso: String) -> HvfWindowsInstallRequest {
        HvfWindowsInstallRequest(isoPath: iso, diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
    }

    private func fixture(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root.appendingPathComponent("request.json"))
    }
}

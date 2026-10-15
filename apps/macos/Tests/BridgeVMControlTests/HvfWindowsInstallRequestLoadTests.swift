import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallRequestLoadTests: XCTestCase {
    func testRequestAtByteLimitStillLoads() throws {
        try fixture { bundle, requestURL, request in
            var bytes = try JSONEncoder().encode(request)
            bytes.append(Data(repeating: 0x20, count: VMLibrary.maximumConfigBytes - bytes.count))
            try bytes.write(to: requestURL)
            XCTAssertTrue(HvfWindowsInstallRequest.load(bundlePath: bundle.path) == request)
        }
    }

    func testOversizedValidRequestIsRefusedWithoutChangingFile() throws {
        try fixture { bundle, requestURL, request in
            var bytes = try JSONEncoder().encode(request)
            bytes.append(Data(repeating: 0x20, count: VMLibrary.maximumConfigBytes + 1 - bytes.count))
            try bytes.write(to: requestURL)
            XCTAssertNil(HvfWindowsInstallRequest.load(bundlePath: bundle.path),
                "Preparation lookup must enforce the same request bound as finalization")
            XCTAssertEqual(try Data(contentsOf: requestURL), bytes)
        }
    }

    func testSymlinkRequestIsRefusedAndItsTargetPreserved() throws {
        try fixture { bundle, requestURL, request in
            let target = bundle.appendingPathComponent("other-request.json")
            let bytes = try JSONEncoder().encode(request)
            try bytes.write(to: target)
            try FileManager.default.createSymbolicLink(at: requestURL, withDestinationURL: target)
            XCTAssertNil(HvfWindowsInstallRequest.load(bundlePath: bundle.path))
            XCTAssertEqual(try Data(contentsOf: target), bytes)
            XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: requestURL.path), target.path)
        }
    }

    func testOversizedSaveIsRefusedWithoutReplacingExistingRequest() throws {
        try fixture { bundle, requestURL, request in
            XCTAssertTrue(request.save(bundlePath: bundle.path))
            let original = try Data(contentsOf: requestURL)
            var oversized = request
            oversized.isoPath = String(repeating: "x", count: VMLibrary.maximumConfigBytes)
            XCTAssertFalse(oversized.save(bundlePath: bundle.path))
            XCTAssertEqual(try Data(contentsOf: requestURL), original)
            XCTAssertTrue(HvfWindowsInstallRequest.load(bundlePath: bundle.path) == request)
        }
    }

    private func fixture(_ body: (URL, URL, HvfWindowsInstallRequest) throws -> Void) throws {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent("install-request-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: bundle.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: bundle) }
        let request = HvfWindowsInstallRequest(isoPath: "/synthetic/windows.iso", diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil)
        try body(bundle, bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName), request)
    }
}

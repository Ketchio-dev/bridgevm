import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class A9ImportRequestTests: XCTestCase {
    private final class Fixture {
        let root: URL
        let requestURL: URL
        var body: [String: Any]

        init() throws {
            let fm = FileManager.default
            root = URL(fileURLWithPath: "/tmp/bridgevm-import-e2e-test-\(UUID().uuidString)")
            let inputs = root.appendingPathComponent("inputs")
            let vtpm = inputs.appendingPathComponent("vtpm")
            let app = root.appendingPathComponent("BridgeVM.app")
            let executable = app.appendingPathComponent("Contents/MacOS/BridgeVMControl")
            let runner = app.appendingPathComponent("Contents/Resources/target/release/hvf-runner")
            for directory in [vtpm, executable.deletingLastPathComponent(), runner.deletingLastPathComponent()] {
                try fm.createDirectory(at: directory, withIntermediateDirectories: true)
            }
            try Data([1]).write(to: executable)
            try Data([2]).write(to: runner)
            let disk = inputs.appendingPathComponent("windows.raw")
            let vars = inputs.appendingPathComponent("vars.fd")
            try Data([3]).write(to: disk)
            fm.createFile(atPath: vars.path, contents: nil)
            let handle = try FileHandle(forWritingTo: vars)
            try handle.truncate(atOffset: 64 * 1024 * 1024); try handle.close()
            try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: disk.path)
            try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: vars.path)
            try fm.setAttributes([.posixPermissions: 0o555], ofItemAtPath: vtpm.path)
            let nonce = String(repeating: "a", count: 64)
            let slug = "bridgevm-a9-import-lane-1-\(nonce.prefix(12))"
            let library = root.appendingPathComponent("library")
            let bundle = library.appendingPathComponent(slug).appendingPathComponent("bundle.vmbridge")
            body = [
                "schema_version": "bridgevm.windows-hvf-import-product-e2e-request.v1",
                "job_id": "import-pilot", "commit": String(repeating: "b", count: 40),
                "campaign_mode": "pilot", "lane": 1, "nonce": nonce,
                "vm_name": "BridgeVM A9 Import Lane 1 \(nonce.prefix(12))", "vm_slug": slug,
                "three_d_injection": false, "app_bundle_path": app.path,
                "app_executable_path": executable.path, "runner_path": runner.path,
                "source_disk_path": disk.path, "source_vars_path": vars.path,
                "source_vtpm_path": vtpm.path, "lane_root": root.path,
                "library_root_path": library.path, "share_path": root.appendingPathComponent("share").path,
                "disk_path": bundle.appendingPathComponent("disks/hvf-target.raw").path,
                "vars_path": bundle.appendingPathComponent("metadata/hvf-vars.fd").path,
                "vtpm_state_path": bundle.appendingPathComponent("metadata/vtpm").path,
                "snapshot_path": bundle.appendingPathComponent("metadata/snapshots/latest.snapshot").path,
                "guest_evidence_path": bundle.appendingPathComponent("metadata/product-e2e-guest-evidence.json").path,
            ]
            requestURL = root.appendingPathComponent("request.json")
            try write()
        }

        deinit { try? FileManager.default.removeItem(at: root) }

        func write() throws {
            let data = try JSONSerialization.data(withJSONObject: body, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: requestURL, options: .atomic)
        }

        func replaceSourceDisk(with url: URL) throws {
            let disk = URL(fileURLWithPath: body["source_disk_path"] as! String)
            try FileManager.default.removeItem(at: disk)
            try FileManager.default.createSymbolicLink(at: disk, withDestinationURL: url)
        }
    }

    func testExactReadOnlyPilotRequestLoads() throws {
        let fixture = try Fixture()
        let request = try A9ImportRequest.load(fixture.requestURL)
        XCTAssertEqual(request.campaignMode, "pilot")
        XCTAssertFalse(request.threeDInjection)
        XCTAssertNotEqual(request.sourceDiskPath, request.diskPath)
    }

    func testUnknownFieldFailsClosed() throws {
        let fixture = try Fixture()
        fixture.body["unexpected"] = true; try fixture.write()
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func test3DEnabledRequestIsRejected() throws {
        let fixture = try Fixture()
        fixture.body["three_d_injection"] = true; try fixture.write()
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testWritableSourceMediaIsRejected() throws {
        let fixture = try Fixture()
        try FileManager.default.setAttributes([.posixPermissions: 0o644],
            ofItemAtPath: fixture.body["source_disk_path"] as! String)
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testWrongVarsSizeIsRejected() throws {
        let fixture = try Fixture()
        let path = fixture.body["source_vars_path"] as! String
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path)
        let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        try handle.truncate(atOffset: 4096); try handle.close()
        try FileManager.default.setAttributes([.posixPermissions: 0o444], ofItemAtPath: path)
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testSymlinkedSourceMediaIsRejected() throws {
        let fixture = try Fixture()
        let target = fixture.root.appendingPathComponent("other.raw")
        try Data([4]).write(to: target)
        try fixture.replaceSourceDisk(with: target)
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }

    func testEscapedLanePathIsRejected() throws {
        let fixture = try Fixture()
        fixture.body["share_path"] = "/tmp/outside-share"; try fixture.write()
        XCTAssertThrowsError(try A9ImportRequest.load(fixture.requestURL))
    }
}

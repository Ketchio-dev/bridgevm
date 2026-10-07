import Foundation
@testable import BridgeVMProductE2E

final class A9ImportRequestFixture {
    let allocation: ProductWorkFixture
    let root: URL
    let requestURL: URL
    var body: [String: Any]
    init() throws {
        let fm = FileManager.default
        allocation = try ProductWorkFixture(job: "import-pilot", importing: true); root = allocation.lane
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
        let package = inputs.appendingPathComponent("vtpm-recovery.json"), code = inputs.appendingPathComponent("vtpm-recovery-code.txt")
        try Data([3]).write(to: disk)
        try Data("package".utf8).write(to: package); try Data("code".utf8).write(to: code)
        fm.createFile(atPath: vars.path, contents: nil)
        let handle = try FileHandle(forWritingTo: vars)
        try handle.truncate(atOffset: 64 * 1024 * 1024); try handle.close()
        try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: disk.path)
        try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: vars.path)
        try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: package.path); try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: code.path)
        try fm.setAttributes([.posixPermissions: 0o555], ofItemAtPath: vtpm.path)
        let nonce = String(repeating: "a", count: 64)
        let slug = "bridgevm-a9-import-lane-1-\(nonce.prefix(12))"
        let library = root.appendingPathComponent("library")
        let bundle = library.appendingPathComponent(slug).appendingPathComponent("bundle")
        body = [
            "schema_version": "bridgevm.windows-hvf-import-product-e2e-request.v1",
            "job_id": "import-pilot", "commit": String(repeating: "b", count: 40),
            "campaign_mode": "pilot", "lane": 1, "nonce": nonce,
            "vm_name": "BridgeVM A9 Import Lane 1 \(nonce.prefix(12))", "vm_slug": slug,
            "three_d_injection": false, "app_bundle_path": app.path,
            "app_executable_path": executable.path, "runner_path": runner.path,
            "source_disk_path": disk.path, "source_vars_path": vars.path,
            "source_vtpm_path": vtpm.path, "source_vtpm_package_path": package.path,
            "source_vtpm_code_path": code.path, "lane_root": root.path,
            "library_root_path": library.path, "share_path": root.appendingPathComponent("share").path,
            "disk_path": bundle.appendingPathComponent("disks/hvf-target.raw").path,
            "vars_path": bundle.appendingPathComponent("metadata/hvf-vars.fd").path,
            "vtpm_state_path": bundle.appendingPathComponent("metadata/vtpm").path,
            "snapshot_path": bundle.appendingPathComponent("metadata/snapshots/latest.snapshot").path,
            "guest_evidence_path": bundle.appendingPathComponent("metadata/product-e2e-guest-evidence.json").path,
        ]
        body.merge(allocation.fields) { _, new in new }; requestURL = root.appendingPathComponent("request.json")
        try write()
    }
    deinit { try? FileManager.default.removeItem(at: allocation.parent) }
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

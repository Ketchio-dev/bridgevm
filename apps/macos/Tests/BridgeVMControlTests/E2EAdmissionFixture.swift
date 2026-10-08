import Foundation
import BridgeVMWindowProtocol

final class E2EAdmissionFixture {
    let parent: URL, work: URL, lane: URL, library: URL, request: URL
    var body: [String: Any]
    init(importing: Bool = false) throws {
        let fm = FileManager.default
        parent = fm.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("e2e-admission-\(UUID().uuidString)")
        work = parent.appendingPathComponent("bridgevm-\(importing ? "import-e2e" : "e2e")-fixture.Ab12Cd")
        lane = work.appendingPathComponent("lane-1"); library = lane.appendingPathComponent("library")
        request = lane.appendingPathComponent("request.json")
        for directory in [parent, work, lane, library] {
            try fm.createDirectory(at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        }
        body = ["schema_version": importing ? "bridgevm.windows-hvf-import-product-e2e-request.v1" : "bridgevm.windows-hvf-3d-off-product-e2e-request.v2",
            "job_id": "fixture", "commit": String(repeating: "a", count: 40), "campaign_mode": "pilot",
            "lane": 1, "nonce": String(repeating: "a", count: 64), "vm_name": "fixture", "vm_slug": "fixture",
            "three_d_injection": false, "work_parent": parent.path,
            "work_parent_identity": try ProductE2EWorkBoundary.identity(parent),
            "work_identity": try ProductE2EWorkBoundary.identity(work),
            "lane_root": lane.path, "library_root_path": library.path]
        let paths = "app_bundle_path app_executable_path runner_path share_path disk_path vars_path vtpm_state_path snapshot_path guest_evidence_path " + (importing ? "source_disk_path source_vars_path source_vtpm_path source_vtpm_package_path source_vtpm_code_path" : "firmware_path secure_boot_policy_path iso_path bundled_vars_seed_path guest_payload_path guest_payload_manifest_path secure_boot_receipt_path")
        for key in paths.split(separator: " ") { body[String(key)] = lane.appendingPathComponent(String(key)).path }
        try write()
    }
    func write() throws { try JSONSerialization.data(withJSONObject: body, options: [.sortedKeys]).write(to: request) }
    deinit { try? FileManager.default.removeItem(at: parent) }
}

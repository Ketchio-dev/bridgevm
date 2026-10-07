import Foundation
#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

/// Private development-only allocation proof; not a user-selected library path.
public enum ProductE2EWorkBoundary {
    public enum Refusal: Error { case invalidAllocation }

    public static func validate(laneRoot: String, parent: String, parentIdentity: String,
                                workIdentity: String, job: String, lane: Int, importing: Bool) throws {
        guard matches(job, #"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"#), (1...3).contains(lane) else {
            throw Refusal.invalidAllocation
        }
        let root = URL(fileURLWithPath: laneRoot, isDirectory: true)
        let work = root.deletingLastPathComponent()
        let container = work.deletingLastPathComponent()
        let prefix = importing ? "bridgevm-import-e2e-" : "bridgevm-e2e-"
        guard root.lastPathComponent == "lane-\(lane)", container.path == parent,
              matches(work.lastPathComponent, "^" + prefix + NSRegularExpression.escapedPattern(for: job) + #"\.[A-Za-z0-9]{6}$"#),
              try identity(container) == parentIdentity, try identity(work) == workIdentity else {
            throw Refusal.invalidAllocation
        }
        let laneIdentity = try identity(root)
        guard parentIdentity.split(separator: ":").first == workIdentity.split(separator: ":").first,
              laneIdentity.split(separator: ":").first == workIdentity.split(separator: ":").first else {
            throw Refusal.invalidAllocation
        }
    }

    public static func identity(_ url: URL) throws -> String {
        guard url.path == url.standardizedFileURL.path,
              url.standardizedFileURL.path == url.resolvingSymlinksInPath().path else {
            throw Refusal.invalidAllocation
        }
        var info = stat()
        guard lstat(url.path, &info) == 0, info.st_mode & mode_t(S_IFMT) == mode_t(S_IFDIR),
              info.st_uid == geteuid(), info.st_mode & 0o777 == 0o700 else {
            throw Refusal.invalidAllocation
        }
        return "\(info.st_dev):\(info.st_ino)"
    }

    public static func validateLibrary(_ library: URL) throws {
        let lane = library.deletingLastPathComponent()
        let request = lane.appendingPathComponent("request.json")
        let values = try request.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 1_048_576 else { throw Refusal.invalidAllocation }
        let data = try Data(contentsOf: request)
        guard let document = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let parent = document["work_parent"] as? String,
              let parentID = document["work_parent_identity"] as? String,
              let workID = document["work_identity"] as? String,
              let job = document["job_id"] as? String,
              let ordinal = document["lane"] as? Int,
              let schema = document["schema_version"] as? String,
              document["lane_root"] as? String == lane.path,
              document["library_root_path"] as? String == library.path,
              library.lastPathComponent == "library",
              schema == "bridgevm.windows-hvf-3d-off-product-e2e-request.v2"
                || schema == "bridgevm.windows-hvf-import-product-e2e-request.v1" else {
            throw Refusal.invalidAllocation
        }
        try exactKeys(data, importing: schema == "bridgevm.windows-hvf-import-product-e2e-request.v1")
        try validate(laneRoot: lane.path, parent: parent, parentIdentity: parentID,
                     workIdentity: workID, job: job, lane: ordinal,
                     importing: schema == "bridgevm.windows-hvf-import-product-e2e-request.v1")
    }

    public static func exactKeys(_ data: Data, importing: Bool) throws {
        guard let text = String(data: data, encoding: .utf8) else { throw Refusal.invalidAllocation }
        let regex = try NSRegularExpression(pattern: #""((?:\\.|[^"\\])*)"\s*:"#)
        let keys = regex.matches(in: text, range: NSRange(text.startIndex..<text.endIndex, in: text)).compactMap {
            Range($0.range(at: 1), in: text).map { String(text[$0]) }
        }
        let common = "schema_version job_id commit campaign_mode lane nonce vm_name vm_slug three_d_injection app_bundle_path app_executable_path runner_path work_parent work_parent_identity work_identity lane_root library_root_path share_path disk_path vars_path vtpm_state_path snapshot_path guest_evidence_path"
        let extra = importing ? "source_disk_path source_vars_path source_vtpm_path source_vtpm_package_path source_vtpm_code_path" : "firmware_path secure_boot_policy_path iso_path bundled_vars_seed_path guest_payload_path guest_payload_manifest_path secure_boot_receipt_path"
        let expected = Set((common + " " + extra).split(separator: " ").map(String.init))
        guard keys.count == expected.count, Set(keys) == expected else { throw Refusal.invalidAllocation }
    }

    private static func matches(_ value: String, _ pattern: String) -> Bool {
        value.range(of: pattern, options: .regularExpression) != nil
    }
}

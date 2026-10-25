import Foundation

extension T17GuestJourney {
    /// The product's snapshot pair and manifest authenticate each other and, once restored, the selected media.
    func verifySnapshot(restored: Bool) throws {
        let root = URL(fileURLWithPath: request.snapshotPath, isDirectory: true)
        let entries = try fileManager.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
        guard Set(entries.map(\.lastPathComponent)) == ["disk.raw", "vars.fd", "manifest.json"],
              let manifest = try JSONSerialization.jsonObject(
                with: Data(contentsOf: root.appendingPathComponent("manifest.json"))) as? [String: Any],
              Set(manifest.keys) == ["format_version", "vm_id", "disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256"] else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot pair or manifest shape is invalid")
        }
        let disk = root.appendingPathComponent("disk.raw"), vars = root.appendingPathComponent("vars.fd")
        let diskSize = try fileSize(disk), varsSize = try fileSize(vars)
        guard manifest["format_version"] as? Int == 1, manifest["vm_id"] as? String == request.vmSlug,
              manifest["disk_bytes"] as? Int == diskSize, manifest["vars_bytes"] as? Int == varsSize,
              manifest["disk_sha256"] as? String == (try T17Evidence.sha256(disk)),
              manifest["vars_sha256"] as? String == (try T17Evidence.sha256(vars)) else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot manifest does not authenticate its pair")
        }
        if restored {
            let selected = try T17SelectedMedia.digest(disk: request.diskPath, vars: request.varsPath)
            if let difference = T17SelectedMedia.difference(selected, manifest: manifest) {
                throw T17Blocker(code: "snapshot-unavailable", detail: difference)
            }
        }
    }

    private func fileSize(_ url: URL) throws -> Int {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true, let size = values.fileSize else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot member is not a regular file")
        }
        return size
    }
}

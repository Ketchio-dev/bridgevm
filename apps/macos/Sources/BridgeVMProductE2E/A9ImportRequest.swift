import Foundation
struct A9ImportRequest: Decodable, Equatable, T17JourneyRequest {
    let schemaVersion: String
    let jobID: String
    let commit: String
    let campaignMode: String
    let lane: Int
    let nonce: String
    let vmName: String
    let vmSlug: String
    let threeDInjection: Bool
    let appBundlePath: String
    let appExecutablePath: String
    let runnerPath: String
    let sourceDiskPath: String
    let sourceVarsPath: String
    let sourceVtpmPath: String; let sourceVtpmPackagePath: String; let sourceVtpmCodePath: String
    let laneRoot: String
    let libraryRootPath: String
    let sharePath: String
    let diskPath: String
    let varsPath: String
    let vtpmStatePath: String
    let snapshotPath: String
    let guestEvidencePath: String

    enum CodingKeys: String, CodingKey, CaseIterable {
        case schemaVersion = "schema_version", jobID = "job_id", commit
        case campaignMode = "campaign_mode", lane, nonce
        case vmName = "vm_name", vmSlug = "vm_slug", threeDInjection = "three_d_injection"
        case appBundlePath = "app_bundle_path", appExecutablePath = "app_executable_path"
        case runnerPath = "runner_path", sourceDiskPath = "source_disk_path"
        case sourceVarsPath = "source_vars_path", sourceVtpmPath = "source_vtpm_path"
        case sourceVtpmPackagePath = "source_vtpm_package_path", sourceVtpmCodePath = "source_vtpm_code_path"
        case laneRoot = "lane_root", libraryRootPath = "library_root_path", sharePath = "share_path"
        case diskPath = "disk_path", varsPath = "vars_path", vtpmStatePath = "vtpm_state_path"
        case snapshotPath = "snapshot_path", guestEvidencePath = "guest_evidence_path"
    }
    static func load(_ url: URL) throws -> Self {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 1_048_576 else {
            throw T17Blocker(code: "invalid-import-request", detail: "request is not a bounded regular file")
        }
        let data = try Data(contentsOf: url)
        try requireExactKeys(data)
        let request = try JSONDecoder().decode(Self.self, from: data)
        try request.validate()
        return request
    }
    func validate(fileManager: FileManager = .default) throws {
        guard schemaVersion == "bridgevm.windows-hvf-import-product-e2e-request.v1",
              Self.matches(jobID, #"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"#),
              Self.matches(commit, #"^[0-9a-f]{40}$"#),
              campaignMode == "pilot" || campaignMode == "release",
              lane >= 1 && lane <= (campaignMode == "release" ? 3 : 1),
              Self.matches(nonce, #"^[0-9a-f]{64}$"#), !threeDInjection,
              vmName == "BridgeVM A9 Import Lane \(lane) \(nonce.prefix(12))",
              vmSlug == "bridgevm-a9-import-lane-\(lane)-\(nonce.prefix(12))" else {
            throw T17Blocker(code: "invalid-import-request", detail: "request identity or 3D policy is invalid")
        }
        let root = URL(fileURLWithPath: laneRoot, isDirectory: true).standardizedFileURL
        guard !(laneRoot as NSString).pathComponents.contains(".."), laneRoot == root.path,
              laneRoot.hasPrefix("/tmp/bridgevm-import-e2e-")
                || laneRoot.hasPrefix("/private/tmp/bridgevm-import-e2e-") else {
            throw T17Blocker(code: "invalid-import-request", detail: "lane root is outside the fixed boundary")
        }
        let inputs = root.appendingPathComponent("inputs", isDirectory: true)
        let library = root.appendingPathComponent("library", isDirectory: true)
        let bundle = library.appendingPathComponent(vmSlug, isDirectory: true)
            .appendingPathComponent("bundle", isDirectory: true)
        let fixed: [(String, URL)] = [
            (sourceDiskPath, inputs.appendingPathComponent("windows.raw")),
            (sourceVarsPath, inputs.appendingPathComponent("vars.fd")),
            (sourceVtpmPath, inputs.appendingPathComponent("vtpm", isDirectory: true)),
            (sourceVtpmPackagePath, inputs.appendingPathComponent("vtpm-recovery.json")), (sourceVtpmCodePath, inputs.appendingPathComponent("vtpm-recovery-code.txt")),
            (libraryRootPath, library), (sharePath, root.appendingPathComponent("share", isDirectory: true)),
            (diskPath, bundle.appendingPathComponent("disks/hvf-target.raw")),
            (varsPath, bundle.appendingPathComponent("metadata/hvf-vars.fd")),
            (vtpmStatePath, bundle.appendingPathComponent("metadata/vtpm", isDirectory: true)),
            (snapshotPath, bundle.appendingPathComponent("metadata/snapshots/latest.snapshot", isDirectory: true)),
            (guestEvidencePath, bundle.appendingPathComponent("metadata/product-e2e-guest-evidence.json")),
        ]
        guard fixed.allSatisfy({ URL(fileURLWithPath: $0.0).standardizedFileURL.path
                == $0.1.standardizedFileURL.path }) else {
            throw T17Blocker(code: "invalid-import-request", detail: "a path escapes its fixed lane name")
        }
        try Self.requireDirectory(root, fileManager: fileManager)
        try Self.requireDirectory(inputs, fileManager: fileManager)
        try Self.requireDirectory(URL(fileURLWithPath: appBundlePath), fileManager: fileManager)
        try Self.requireReadOnlyTree(URL(fileURLWithPath: sourceVtpmPath), fileManager: fileManager)
        try Self.requireFile(URL(fileURLWithPath: sourceVtpmPackagePath), readOnly: true, fileManager: fileManager); try Self.requireFile(URL(fileURLWithPath: sourceVtpmCodePath), readOnly: true, fileManager: fileManager)
        try Self.requireFile(URL(fileURLWithPath: appExecutablePath), fileManager: fileManager)
        try Self.requireFile(URL(fileURLWithPath: runnerPath), fileManager: fileManager)
        try Self.requireFile(URL(fileURLWithPath: sourceDiskPath), readOnly: true, fileManager: fileManager)
        try Self.requireFile(URL(fileURLWithPath: sourceVarsPath), exactBytes: 64 * 1024 * 1024,
                        readOnly: true, fileManager: fileManager)
        let app = URL(fileURLWithPath: appBundlePath).standardizedFileURL
        guard URL(fileURLWithPath: appExecutablePath).standardizedFileURL
                == app.appendingPathComponent("Contents/MacOS/BridgeVMControl").standardizedFileURL,
              URL(fileURLWithPath: runnerPath).standardizedFileURL
                == app.appendingPathComponent("Contents/Resources/target/release/hvf-runner").standardizedFileURL,
              sourceDiskPath != diskPath, sourceVarsPath != varsPath, sourceVtpmPath != vtpmStatePath else {
            throw T17Blocker(code: "invalid-import-request", detail: "artifact relation or media ownership is invalid")
        }
    }

    private static func requireExactKeys(_ data: Data) throws {
        guard let text = String(data: data, encoding: .utf8),
              let regex = try? NSRegularExpression(pattern: #""((?:\\.|[^"\\])*)"\s*:"#) else {
            throw T17Blocker(code: "invalid-import-request", detail: "request is not UTF-8 JSON")
        }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        let keys = regex.matches(in: text, range: range).compactMap { match in
            Range(match.range(at: 1), in: text).map { String(text[$0]) }
        }
        let expected = Set(CodingKeys.allCases.map(\.rawValue))
        guard keys.count == expected.count, Set(keys) == expected else {
            throw T17Blocker(code: "invalid-import-request", detail: "request has missing, duplicate, or unknown fields")
        }
    }

    private static func requireDirectory(_ url: URL, fileManager: FileManager) throws {
        let values = try url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw T17Blocker(code: "invalid-import-request", detail: "required directory is missing or unsafe")
        }
    }

    private static func requireFile(_ url: URL, exactBytes: Int? = nil, readOnly: Bool = false,
                                    fileManager: FileManager) throws {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        let mode = (try fileManager.attributesOfItem(atPath: url.path)[.posixPermissions] as? NSNumber)?.intValue
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, exactBytes == nil || size == exactBytes,
              !readOnly || (mode.map { $0 & 0o222 == 0 } == true) else {
            throw T17Blocker(code: "invalid-import-request", detail: "required file is missing, unsafe, or malformed")
        }
    }

    private static func requireReadOnlyTree(_ root: URL, fileManager: FileManager) throws {
        try requireDirectory(root, fileManager: fileManager)
        let rootMode = (try fileManager.attributesOfItem(atPath: root.path)[.posixPermissions] as? NSNumber)?.intValue
        guard rootMode.map({ $0 & 0o222 == 0 }) == true,
              let entries = fileManager.enumerator(at: root, includingPropertiesForKeys:
                [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey]) else {
            throw T17Blocker(code: "invalid-import-request", detail: "source vTPM tree is writable or unreadable")
        }
        var count = 0
        for case let item as URL in entries {
            count += 1
            guard count <= 1_024 else {
                throw T17Blocker(code: "invalid-import-request", detail: "source vTPM tree is oversized")
            }
            let values = try item.resourceValues(forKeys: [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey])
            let mode = (try fileManager.attributesOfItem(atPath: item.path)[.posixPermissions] as? NSNumber)?.intValue
            guard values.isSymbolicLink != true, values.isRegularFile == true || values.isDirectory == true,
                  mode.map({ $0 & 0o222 == 0 }) == true else {
                throw T17Blocker(code: "invalid-import-request", detail: "source vTPM tree contains an unsafe entry")
            }
        }
    }

    private static func matches(_ value: String, _ pattern: String) -> Bool {
        value.range(of: pattern, options: .regularExpression) != nil
    }
}

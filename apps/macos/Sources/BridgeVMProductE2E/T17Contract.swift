import CryptoKit
import Foundation

struct T17Blocker: Error, Equatable {
    let code: String
    let detail: String
}

struct T17Request: Decodable, Equatable {
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
    let firmwarePath: String
    let secureBootPolicyPath: String
    let isoPath: String
    let bundledVarsSeedPath: String
    let guestPayloadPath: String
    let guestPayloadManifestPath: String
    let laneRoot: String
    let libraryRootPath: String
    let sharePath: String
    let diskPath: String
    let varsPath: String
    let vtpmStatePath: String
    let snapshotPath: String
    let secureBootReceiptPath: String
    let guestEvidencePath: String

    enum CodingKeys: String, CodingKey, CaseIterable {
        case schemaVersion = "schema_version"
        case jobID = "job_id"
        case commit, campaignMode = "campaign_mode", lane, nonce
        case vmName = "vm_name", vmSlug = "vm_slug"
        case threeDInjection = "three_d_injection"
        case appBundlePath = "app_bundle_path"
        case appExecutablePath = "app_executable_path"
        case runnerPath = "runner_path"
        case firmwarePath = "firmware_path"
        case secureBootPolicyPath = "secure_boot_policy_path"
        case isoPath = "iso_path"
        case bundledVarsSeedPath = "bundled_vars_seed_path"
        case guestPayloadPath = "guest_payload_path"
        case guestPayloadManifestPath = "guest_payload_manifest_path"
        case laneRoot = "lane_root"
        case libraryRootPath = "library_root_path"
        case sharePath = "share_path"
        case diskPath = "disk_path"
        case varsPath = "vars_path"
        case vtpmStatePath = "vtpm_state_path"
        case snapshotPath = "snapshot_path"
        case secureBootReceiptPath = "secure_boot_receipt_path"
        case guestEvidencePath = "guest_evidence_path"
    }

    static func load(_ url: URL) throws -> T17Request {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 1_048_576 else {
            throw T17Blocker(code: "invalid-request", detail: "request is not a bounded regular file")
        }
        let data = try Data(contentsOf: url)
        try requireExactTopLevelKeys(data)
        let request = try JSONDecoder().decode(T17Request.self, from: data)
        try request.validate()
        return request
    }

    private static func requireExactTopLevelKeys(_ data: Data) throws {
        guard let text = String(data: data, encoding: .utf8) else {
            throw T17Blocker(code: "invalid-request", detail: "request is not UTF-8")
        }
        let pattern = #""((?:\\.|[^"\\])*)"\s*:"#
        let regex = try NSRegularExpression(pattern: pattern)
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        let keys = regex.matches(in: text, range: range).compactMap { match -> String? in
            guard let swiftRange = Range(match.range(at: 1), in: text) else { return nil }
            return String(text[swiftRange])
        }
        let expected = Set(CodingKeys.allCases.map(\.rawValue))
        guard keys.count == expected.count, Set(keys) == expected else {
            throw T17Blocker(code: "invalid-request", detail: "request has missing, duplicate, or unknown fields")
        }
    }
    func validate(fileManager: FileManager = .default) throws {
        guard schemaVersion == "bridgevm.windows-hvf-3d-off-product-e2e-request.v2",
              Self.matches(jobID, #"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"#),
              Self.matches(commit, #"^[0-9a-f]{40}$"#),
              campaignMode == "pilot" || campaignMode == "release",
              lane >= 1 && lane <= (campaignMode == "release" ? 3 : 1),
              Self.matches(nonce, #"^[0-9a-f]{64}$"#), !threeDInjection,
              vmName == "BridgeVM T17 Lane \(lane) \(nonce.prefix(12))",
              vmSlug == "bridgevm-t17-lane-\(lane)-\(nonce.prefix(12))" else {
            throw T17Blocker(code: "invalid-request", detail: "request identity or 3D policy is invalid")
        }
        let root = URL(fileURLWithPath: laneRoot, isDirectory: true).standardizedFileURL
        try T17StorageBoundary.validate(laneRoot)
        guard !(laneRoot as NSString).pathComponents.contains(".."), laneRoot == root.path else {
            throw T17Blocker(code: "invalid-request", detail: "lane root is outside the fixed temporary boundary")
        }
        let library = root.appendingPathComponent("library", isDirectory: true)
        let bundle = library.appendingPathComponent(vmSlug, isDirectory: true)
            .appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let fixed: [(String, URL)] = [
            (libraryRootPath, library), (sharePath, root.appendingPathComponent("share", isDirectory: true)),
            (diskPath, bundle.appendingPathComponent("disks/hvf-target.raw")),
            (varsPath, bundle.appendingPathComponent("metadata/hvf-vars.fd")),
            (vtpmStatePath, bundle.appendingPathComponent("metadata/vtpm", isDirectory: true)),
            (snapshotPath, bundle.appendingPathComponent("metadata/snapshots/latest.snapshot", isDirectory: true)),
            (secureBootReceiptPath, bundle.appendingPathComponent("metadata/secure-boot-provisioning.json")),
            (guestEvidencePath, bundle.appendingPathComponent("metadata/product-e2e-guest-evidence.json")),
        ]
        guard fixed.allSatisfy({ URL(fileURLWithPath: $0.0).standardizedFileURL.path == $0.1.standardizedFileURL.path }),
              !fixed.isEmpty else {
            throw T17Blocker(code: "invalid-request", detail: "a writable path escapes its fixed lane name")
        }
        try requireDirectory(root, code: "invalid-request", fileManager: fileManager)
        try requireDirectory(URL(fileURLWithPath: appBundlePath), code: "app-launch-failed", fileManager: fileManager)
        try requireDirectory(URL(fileURLWithPath: guestPayloadPath), code: "missing-guest-payload", fileManager: fileManager)
        for path in [appExecutablePath, runnerPath, firmwarePath, secureBootPolicyPath, isoPath,
                     bundledVarsSeedPath, guestPayloadManifestPath] {
            try requireFile(URL(fileURLWithPath: path), code: path == guestPayloadManifestPath ? "missing-guest-payload" : "invalid-request")
        }
        let appBundle = URL(fileURLWithPath: appBundlePath)
        guard URL(fileURLWithPath: appExecutablePath).standardizedFileURL
                == appBundle.appendingPathComponent("Contents/MacOS/BridgeVMControl").standardizedFileURL,
              URL(fileURLWithPath: runnerPath).standardizedFileURL
                == appBundle.appendingPathComponent("Contents/Resources/target/release/hvf-runner").standardizedFileURL else {
            throw T17Blocker(code: "invalid-request", detail: "packaged executable relation is invalid")
        }
        let payload = URL(fileURLWithPath: guestPayloadPath).resolvingSymlinksInPath().standardizedFileURL
        let manifest = URL(fileURLWithPath: guestPayloadManifestPath).resolvingSymlinksInPath().standardizedFileURL
        guard manifest.path != payload.path && !manifest.path.hasPrefix(payload.path + "/") else {
            throw T17Blocker(code: "missing-guest-payload", detail: "payload manifest must be outside the payload tree")
        }
        guard !fileManager.fileExists(atPath: libraryRootPath), !fileManager.fileExists(atPath: sharePath) else {
            throw T17Blocker(code: "invalid-request", detail: "lane library and share outputs must be absent")
        }
    }

    private func requireDirectory(_ url: URL, code: String, fileManager: FileManager) throws {
        let values = try url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw T17Blocker(code: code, detail: "required directory is missing or unsafe")
        }
    }

    private func requireFile(_ url: URL, code: String) throws {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true else {
            throw T17Blocker(code: code, detail: "required file is missing or unsafe")
        }
    }

    private static func matches(_ value: String, _ pattern: String) -> Bool {
        value.range(of: pattern, options: .regularExpression) != nil
    }
}

enum T17Stage: String, CaseIterable {
    case artifactPreflight = "artifact_preflight"
    case vmCreated = "vm_created"
    case sourcePrepared = "source_prepared"
    case windowsInstalled = "windows_installed"
    case secureBootProvisioned = "secure_boot_provisioned"
    case firstReady = "first_ready"
    case keyboardPointer = "keyboard_pointer"
    case clipboard
    case folderShare = "folder_share"
    case network, audio
    case firstShutdown = "first_shutdown"
    case snapshotRestore = "snapshot_restore"
    case secondReady = "second_ready"
    case secondShutdown = "second_shutdown"
}

struct T17LaneResult: Encodable {
    let schemaVersion = "bridgevm.windows-hvf-3d-off-product-e2e-lane.v2"
    let jobID: String
    let commit: String
    let campaignMode: String
    let lane: Int
    let nonce: String
    let threeDInjection = false
    let uiFrontendAutomated: Bool
    let failureCode: String
    let cleanupVerified: Bool
    let installerSourcePath: String
    let stages: [T17Stage: Bool]
    let hashes: [String: String]

    func encode(to encoder: Encoder) throws {
        var output = encoder.container(keyedBy: DynamicKey.self)
        try output.encode(schemaVersion, forKey: .init("schema_version"))
        try output.encode(jobID, forKey: .init("job_id")); try output.encode(commit, forKey: .init("commit"))
        try output.encode(campaignMode, forKey: .init("campaign_mode")); try output.encode(lane, forKey: .init("lane"))
        try output.encode(nonce, forKey: .init("nonce")); try output.encode(threeDInjection, forKey: .init("three_d_injection"))
        try output.encode(uiFrontendAutomated, forKey: .init("ui_frontend_automated"))
        try output.encode(failureCode, forKey: .init("failure_code")); try output.encode(cleanupVerified, forKey: .init("cleanup_verified"))
        try output.encode(installerSourcePath, forKey: .init("installer_source_path"))
        for stage in T17Stage.allCases { try output.encode(stages[stage] == true, forKey: .init(stage.rawValue)) }
        for field in T17Evidence.hashFields { try output.encode(hashes[field]!, forKey: .init(field)) }
    }
}

struct DynamicKey: CodingKey {
    let stringValue: String
    let intValue: Int? = nil
    init(_ value: String) { stringValue = value }
    init?(stringValue: String) { self.init(stringValue) }
    init?(intValue: Int) { return nil }
}

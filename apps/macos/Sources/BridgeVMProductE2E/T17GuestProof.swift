import CryptoKit
import Foundation

struct T17RunLogProof {
    let sha256: String
    let readyOffset: Int
    let readyLineHash: String
    let shutdownOffset: Int
    let shutdownLineHash: String

    static func capture(_ url: URL, nonce: String, readyTag: String, shutdownTag: String) throws -> Self {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 64 * 1024 * 1024 else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "run log is missing, unsafe, or over 64 MiB")
        }
        let data = try Data(contentsOf: url, options: [.mappedIfSafe])
        let lines = records(data)
        guard let ready = lines.first(where: { $0.line.hasPrefix("BVAGENT READY") }),
              let shutdown = lines.last(where: { $0.line.hasPrefix("stop: PSCI SYSTEM_OFF") }),
              ready.offset < shutdown.offset else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "run log lacks ordered READY and SYSTEM_OFF records")
        }
        return Self(
            sha256: digest(data), readyOffset: ready.offset,
            readyLineHash: lineHash(tag: readyTag, nonce: nonce, line: ready.line),
            shutdownOffset: shutdown.offset,
            shutdownLineHash: lineHash(tag: shutdownTag, nonce: nonce, line: shutdown.line))
    }

    static func audioPassed(_ url: URL) -> Bool {
        guard let data = try? Data(contentsOf: url, options: [.mappedIfSafe]),
              let line = records(data).last(where: { $0.line.hasPrefix("hda CoreAudio stats:") })?.line else { return false }
        func value(_ key: String) -> Int? {
            guard let range = line.range(of: "\(key)=") else { return nil }
            return Int(line[range.upperBound...].prefix(while: \.isNumber))
        }
        return (value("frames_rendered") ?? 0) > 0 && value("drops") == 0 && value("callback_errors") == 0
    }

    private static func records(_ data: Data) -> [(offset: Int, line: String)] {
        var output: [(Int, String)] = [], start = 0
        for end in 0..<data.count where data[end] == 0x0a {
            var finish = end
            if finish > start && data[finish - 1] == 0x0d { finish -= 1 }
            if finish > start { output.append((start, String(decoding: data[start..<finish], as: UTF8.self))) }
            start = end + 1
        }
        return output
    }

    private static func lineHash(tag: String, nonce: String, line: String) -> String {
        digest(Data("bridgevm-t17-\(tag)-v1\n\(nonce)\n\(line)\n".utf8))
    }

    private static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

struct T17GuestArtifacts {
    static let observationKeys = [
        "keyboard_pointer_challenge_sha256", "clipboard_roundtrip_sha256",
        "share_host_to_guest_sha256", "share_guest_to_host_sha256",
        "network_result_sha256", "audio_result_sha256", "audio_playback_count",
        "audio_error_count", "snapshot_marker_a_sha256", "snapshot_marker_b_sha256",
        "snapshot_marker_restored_a_sha256",
    ]

    let hashes: [String: String]
    let audioPlaybackCount: Int
    let audioErrorCount: Int
    let agentResultSHA256: String

    static func collect(request: T17Request, fileManager: FileManager) throws -> Self {
        let prefix = String(request.nonce.prefix(12))
        let share = URL(fileURLWithPath: request.sharePath, isDirectory: true)
        let bodies = [
            "keyboard_pointer_challenge_sha256": ("t17-keyboard-pointer-\(prefix).txt", "bridgevm-t17-keyboard-pointer-v1\n\(request.nonce)\n"),
            "clipboard_roundtrip_sha256": ("t17-clipboard-guest-\(prefix).txt", "브리지VM T17 클립보드 왕복 v1\n\(request.nonce)\n"),
            "share_host_to_guest_sha256": ("t17-\(prefix).txt", "bridgevm-t17-share-v1\n\(request.nonce)\n"),
            "share_guest_to_host_sha256": ("t17-guest-\(prefix).txt", "bridgevm-t17-guest-share-v1\n\(request.nonce)\n"),
            "network_result_sha256": ("t17-network-\(prefix).txt", "bridgevm-t17-network-ok-v1\n\(request.nonce)\n"),
            "audio_result_sha256": ("t17-audio-\(prefix).txt", "bridgevm-t17-audio-ok-v1\n\(request.nonce)\n"),
            "snapshot_marker_a_sha256": ("t17-snapshot-a-\(prefix).txt", "bridgevm-t17-snapshot-a-v1\n\(request.nonce)\n"),
            "snapshot_marker_b_sha256": ("t17-snapshot-b-\(prefix).txt", "bridgevm-t17-snapshot-b-v1\n\(request.nonce)\n"),
            "snapshot_marker_restored_a_sha256": ("t17-snapshot-restored-a-\(prefix).txt", "bridgevm-t17-snapshot-a-v1\n\(request.nonce)\n"),
        ]
        var hashes: [String: String] = [:]
        for (key, proof) in bodies {
            let url = share.appendingPathComponent(proof.0)
            guard try Data(contentsOf: url) == Data(proof.1.utf8) else {
                throw T17Blocker(code: "guest-evidence-missing", detail: "guest observation bytes differ: \(proof.0)")
            }
            hashes[key] = try T17Evidence.sha256(url)
        }
        let source = share.appendingPathComponent("t17-agent-result-\(prefix).json")
        let data = try Data(contentsOf: source)
        let identityKeys = ["schema_version", "job_id", "commit", "lane", "nonce", "vm_slug"]
        let expectedKeys = Set(identityKeys).union(observationKeys)
        guard exactKeys(data) == expectedKeys,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest agent-result has a non-exact schema")
        }
        guard object["schema_version"] as? String == "bridgevm.windows-product-e2e-agent-result.v2",
              object["job_id"] as? String == request.jobID,
              object["commit"] as? String == request.commit,
              object["lane"] as? Int == request.lane,
              object["nonce"] as? String == request.nonce,
              object["vm_slug"] as? String == request.vmSlug,
              observationKeys.filter({ $0.hasSuffix("_sha256") }).allSatisfy({ object[$0] as? String == hashes[$0] }),
              object["audio_playback_count"] as? Int == 1,
              object["audio_error_count"] as? Int == 0 else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest agent-result observations do not authenticate raw files")
        }
        let destination = URL(fileURLWithPath: request.guestEvidencePath).deletingLastPathComponent()
            .appendingPathComponent("product-e2e/agent-result.json")
        try fileManager.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: destination, options: [.withoutOverwriting])
        guard try T17Evidence.sha256(destination) == T17Evidence.sha256(source) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "managed agent-result copy differs")
        }
        return Self(hashes: hashes, audioPlaybackCount: 1, audioErrorCount: 0,
                    agentResultSHA256: try T17Evidence.sha256(destination))
    }

    func write(request: T17Request, first: T17RunLogProof,
               mutation: T17RunLogProof, final: T17RunLogProof) throws {
        var body: [String: Any] = [
            "schema_version": "bridgevm.windows-product-e2e-guest-evidence.v2",
            "job_id": request.jobID, "commit": request.commit, "lane": request.lane,
            "nonce": request.nonce, "vm_slug": request.vmSlug,
            "first_run_log_sha256": first.sha256, "final_run_log_sha256": final.sha256,
            "mutation_run_log_sha256": mutation.sha256,
            "agent_result_sha256": agentResultSHA256,
            "first_ready_offset": first.readyOffset, "first_ready_line_nonce_sha256": first.readyLineHash,
            "final_ready_offset": final.readyOffset, "final_ready_line_nonce_sha256": final.readyLineHash,
            "first_shutdown_offset": first.shutdownOffset, "first_shutdown_line_nonce_sha256": first.shutdownLineHash,
            "mutation_ready_offset": mutation.readyOffset, "mutation_ready_line_nonce_sha256": mutation.readyLineHash,
            "mutation_shutdown_offset": mutation.shutdownOffset, "mutation_shutdown_line_nonce_sha256": mutation.shutdownLineHash,
            "second_shutdown_offset": final.shutdownOffset, "second_shutdown_line_nonce_sha256": final.shutdownLineHash,
            "audio_playback_count": audioPlaybackCount, "audio_error_count": audioErrorCount,
        ]
        for (key, value) in hashes { body[key] = value }
        let data = try JSONSerialization.data(withJSONObject: body, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: URL(fileURLWithPath: request.guestEvidencePath), options: [.withoutOverwriting])
    }

    private static func exactKeys(_ data: Data) -> Set<String>? {
        guard let text = String(data: data, encoding: .utf8),
              let regex = try? NSRegularExpression(pattern: #"\"((?:\\.|[^\"\\])*)\"\s*:"#) else { return nil }
        let matches = regex.matches(in: text, range: NSRange(text.startIndex..<text.endIndex, in: text))
        let keys = matches.compactMap { Range($0.range(at: 1), in: text).map { String(text[$0]) } }
        return keys.count == Set(keys).count ? Set(keys) : nil
    }
}

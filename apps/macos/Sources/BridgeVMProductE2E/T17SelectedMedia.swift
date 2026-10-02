import Foundation

/// The restored pair is checked through the product's own selection. After a restore the
/// original disk and vars paths keep their old bytes and the selected generation lives under
/// the managed root, so hashing the originals failed a correct restore (Studio T17 r79).
enum T17SelectedMedia {
    struct Digest: Equatable { var diskBytes: Int; var diskSHA256: String; var varsBytes: Int; var varsSHA256: String }

    /// `snapshot_pair_cli` ships in the product app beside this helper (Contents/Helpers/<helper>.app).
    static func cli(helperBundle: URL = Bundle.main.bundleURL) -> URL {
        helperBundle.deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Resources/target/release/examples/snapshot_pair_cli")
    }

    static func digest(disk: String, vars: String, run: (URL, [String]) throws -> String = output) throws -> Digest {
        guard let digest = parse(try run(cli(), ["digest", disk, vars])) else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "selected media digest was unreadable")
        }
        return digest
    }

    static func parse(_ text: String) -> Digest? {
        var fields: [String: String] = [:]
        for line in text.split(separator: "\n") {
            let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
            guard parts.count == 2, fields[parts[0]] == nil else { return nil }
            fields[parts[0]] = parts[1]
        }
        guard fields.count == 4, let diskBytes = fields["disk_bytes"].flatMap({ Int($0) }),
              let varsBytes = fields["vars_bytes"].flatMap({ Int($0) }),
              let disk = fields["disk_sha256"], let vars = fields["vars_sha256"],
              [disk, vars].allSatisfy({ $0.count == 64 && $0.allSatisfy(\.isHexDigit) }) else { return nil }
        return Digest(diskBytes: diskBytes, diskSHA256: disk, varsBytes: varsBytes, varsSHA256: vars)
    }

    /// Compared with the manifest already authenticated against the snapshot files.
    static func difference(_ selected: Digest, manifest: [String: Any]) -> String? {
        let disk = selected.diskSHA256 == manifest["disk_sha256"] as? String && selected.diskBytes == manifest["disk_bytes"] as? Int
        let vars = selected.varsSHA256 == manifest["vars_sha256"] as? String && selected.varsBytes == manifest["vars_bytes"] as? Int
        if disk && vars { return nil }
        return "restored media differs from the snapshot pair; disk=\(disk ? "match" : "differs") vars=\(vars ? "match" : "differs")"
    }

    /// Hashing 64 GiB takes about a minute; a helper that never exits is ended at 15 minutes.
    static func output(_ executable: URL, _ arguments: [String]) throws -> String {
        let process = Process(), pipe = Pipe()
        process.executableURL = executable; process.arguments = arguments
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardOutput = pipe; process.standardError = FileHandle.nullDevice
        try process.run()
        let watchdog = DispatchWorkItem { if process.isRunning { process.terminate() } }
        DispatchQueue.global().asyncAfter(deadline: .now() + 900, execute: watchdog)
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit(); watchdog.cancel()
        guard process.terminationReason == .exit, process.terminationStatus == 0 else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "selected media digest helper failed status=\(process.terminationStatus)")
        }
        return String(decoding: data, as: UTF8.self)
    }
}

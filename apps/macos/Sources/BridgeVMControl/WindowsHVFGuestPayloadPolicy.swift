import Foundation

enum WindowsHVFGuestPayloadPolicy {
    struct StagedPayload {
        let directory: String
        let manifest: String
        let identity: String
    }

    static func stage(
        payloadDirectory: String,
        manifestPath: String,
        in bundlePath: String
    ) -> StagedPayload? {
        let before = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: payloadDirectory, manifestPath: manifestPath)
        guard before.error == nil else { return nil }

        let fm = FileManager.default
        let metadata = URL(fileURLWithPath: bundlePath).appendingPathComponent("metadata")
        let destination = metadata.appendingPathComponent("windows-guest-payload", isDirectory: true)
        let manifestDestination = metadata.appendingPathComponent("windows-guest-payload.tsv")
        guard !fm.fileExists(atPath: destination.path),
              !fm.fileExists(atPath: manifestDestination.path) else { return nil }
        do {
            try fm.copyItem(at: URL(fileURLWithPath: payloadDirectory), to: destination)
            try fm.copyItem(at: URL(fileURLWithPath: manifestPath), to: manifestDestination)
        } catch {
            try? fm.removeItem(at: destination)
            try? fm.removeItem(at: manifestDestination)
            return nil
        }
        let after = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: destination.path, manifestPath: manifestDestination.path)
        guard after.error == nil, after.digest == before.digest else {
            try? fm.removeItem(at: destination)
            try? fm.removeItem(at: manifestDestination)
            return nil
        }
        return StagedPayload(
            directory: destination.path, manifest: manifestDestination.path, identity: after.digest)
    }
}

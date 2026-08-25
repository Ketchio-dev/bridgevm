import Foundation

extension HvfWindowsKernelPolicyVerifier {
    static func stageVerifiedSnapshot(
        from source: URL,
        to destination: URL,
        now: Date = Date(),
        trustAnchors: [TrustAnchor] = productionTrustAnchors,
        copyFile: ((URL, URL) throws -> Void)? = nil
    ) -> Result<VerifiedPackage, Failure> {
        let initial: VerifiedPackage
        switch verify(packageDirectory: source, now: now, trustAnchors: trustAnchors) {
        case .success(let package): initial = package
        case .failure(let failure): return .failure(failure)
        }
        let fm = FileManager.default
        let parent = destination.deletingLastPathComponent()
        guard HvfPrivateSnapshotPath.isPrivateParent(parent),
              !destination.lastPathComponent.isEmpty else {
            return .failure(.snapshotInvalid)
        }
        let temporary = parent.appendingPathComponent(
            ".\(destination.lastPathComponent).verifying.\(UUID().uuidString)", isDirectory: true)
        guard !HvfPrivateSnapshotPath.entryExists(destination),
              !HvfPrivateSnapshotPath.entryExists(temporary) else {
            return .failure(.snapshotInvalid)
        }
        do {
            try fm.createDirectory(at: temporary, withIntermediateDirectories: false,
                                   attributes: [.posixPermissions: 0o700])
            for name in initial.attestation.artifacts.map(\.fileName)
                + [attestationName, signatureName] {
                let input = source.appendingPathComponent(name)
                let output = temporary.appendingPathComponent(name)
                if let copyFile { try copyFile(input, output) }
                else { try fm.copyItem(at: input, to: output) }
            }
            let copied: VerifiedPackage
            switch verify(packageDirectory: temporary, now: now, trustAnchors: trustAnchors) {
            case .success(let package): copied = package
            case .failure: throw Failure.snapshotInvalid
            }
            guard copied == initial, HvfPrivateSnapshotPath.isPrivateParent(parent),
                  !HvfPrivateSnapshotPath.entryExists(destination) else {
                throw Failure.snapshotInvalid
            }
            try fm.moveItem(at: temporary, to: destination)
            return .success(copied)
        } catch {
            try? fm.removeItem(at: temporary)
            return .failure(.snapshotInvalid)
        }
    }
}

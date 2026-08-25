import Foundation

/// A verified package copy made before any VM destination is reserved.
/// The scratch directory is private and is removed unless publication succeeds.
final class HvfWindowsPreparedPackageSnapshot {
    static let bundleRelativePath = "metadata/hvf-injection/package"

    let verified: HvfWindowsKernelPolicyVerifier.VerifiedPackage
    private let root: URL
    private let snapshot: URL
    private let now: Date
    private let trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor]

    private init(
        verified: HvfWindowsKernelPolicyVerifier.VerifiedPackage,
        root: URL,
        snapshot: URL,
        now: Date,
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor]
    ) {
        self.verified = verified
        self.root = root
        self.snapshot = snapshot
        self.now = now
        self.trustAnchors = trustAnchors
    }

    deinit { try? FileManager.default.removeItem(at: root) }

    static func prepare(
        source: URL,
        now: Date = Date(),
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor] =
            HvfWindowsKernelPolicyVerifier.productionTrustAnchors
    ) -> Result<HvfWindowsPreparedPackageSnapshot, HvfWindowsKernelPolicyVerifier.Failure> {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(
            "bridgevm-kernel-policy-\(UUID().uuidString)", isDirectory: true)
        do {
            try fm.createDirectory(
                at: root, withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700])
        } catch {
            return .failure(.snapshotInvalid)
        }
        let snapshot = root.appendingPathComponent("package", isDirectory: true)
        switch HvfWindowsKernelPolicyVerifier.stageVerifiedSnapshot(
            from: source, to: snapshot, now: now, trustAnchors: trustAnchors
        ) {
        case .success(let verified):
            return .success(.init(
                verified: verified, root: root, snapshot: snapshot,
                now: now, trustAnchors: trustAnchors))
        case .failure(let failure):
            try? fm.removeItem(at: root)
            return .failure(failure)
        }
    }

    /// Re-copy and reverify into the bundle so external-volume publication has
    /// the same verify-copy-verify boundary as same-volume publication.
    func publish(bundle: URL) -> Result<URL, HvfWindowsKernelPolicyVerifier.Failure> {
        let fm = FileManager.default
        let parent = bundle.appendingPathComponent("metadata/hvf-injection", isDirectory: true)
        guard !HvfPrivateSnapshotPath.entryExists(parent) else {
            return .failure(.snapshotInvalid)
        }
        do {
            try fm.createDirectory(
                at: parent, withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700])
        } catch {
            return .failure(.snapshotInvalid)
        }
        let destination = parent.appendingPathComponent("package", isDirectory: true)
        switch HvfWindowsKernelPolicyVerifier.stageVerifiedSnapshot(
            from: snapshot, to: destination, now: now, trustAnchors: trustAnchors
        ) {
        case .success(let copied) where copied == verified:
            return .success(destination)
        case .success, .failure:
            try? fm.removeItem(at: parent)
            return .failure(.snapshotInvalid)
        }
    }
}

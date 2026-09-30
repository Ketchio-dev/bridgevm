import Darwin
import Foundation

/// Scripted-install media live in `<bundle>/metadata/hvf-install-staging`: a 0700
/// directory this user creates for each fresh attempt. Fixed names in a shared
/// temporary directory could be pre-created, linked or read by other local users.
enum HvfWindowsInstallStaging {
    static let directoryName = "hvf-install-staging"
    static let targetName = "target.raw", varsName = "vars.fd", evidenceName = "evidence"

    static func root(bundle: URL) -> URL {
        bundle.appendingPathComponent("metadata/\(directoryName)", isDirectory: true)
    }

    /// Fresh attempts only: the pipeline has just proven no finalization journal owns these inputs.
    static func prepare(_ plan: HvfWindowsInstallPlan) throws {
        let seed = try HvfWindowsBootSeed.bundledSeed()
        let metadata = try HvfWindowsInstallPrivateDirectory.metadata(of: URL(fileURLWithPath: plan.bundlePath))
        try metadata.removeTree(directoryName)
        let staging = try metadata.makeDirectory(directoryName)
        _ = try staging.makeDirectory(evidenceName)
        try staging.createFile(varsName) { try $0.write(contentsOf: seed) }
        try staging.createFile(targetName) { try $0.truncate(atOffset: plan.freshTargetSizeBytes) }
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var url = staging.url
        try? url.setResourceValues(values)
    }

    /// Cancelled or failed fresh attempts drop the large media; the evidence stays for diagnosis.
    static func discardMedia(_ plan: HvfWindowsInstallPlan) throws {
        guard let staging = try HvfWindowsInstallPrivateDirectory
            .metadata(of: URL(fileURLWithPath: plan.bundlePath)).existingDirectory(directoryName) else { return }
        for name in [targetName, varsName] { try staging.unlink(name) }
    }

    /// After commit the bundle keeps only logs/install-run.log.
    static func discard(_ paths: HvfWindowsInstallFinalizationPaths) throws {
        try HvfWindowsInstallPrivateDirectory.metadata(of: paths.bundle).removeTree(directoryName)
    }

    /// A source already cloned into the transaction. This bundle's staged media are
    /// unlinked through the staging directory's descriptor, never through its path.
    static func consume(_ source: URL, paths: HvfWindowsInstallFinalizationPaths) throws {
        guard [paths.stagingDisk.path, paths.stagingVars.path].contains(source.path) else {
            return try HvfWindowsInstallDurability.durableRemove(source)
        }
        try HvfWindowsInstallPrivateDirectory.metadata(of: paths.bundle)
            .existingDirectory(directoryName)?.unlink(source.lastPathComponent)
    }

    /// Current journals name this bundle's staging media. Journals sealed by an
    /// earlier build name its fixed shared-/tmp files and remain resumable.
    static func admitsJournalSources(_ journal: HvfWindowsInstallFinalizationJournal,
                                     paths: HvfWindowsInstallFinalizationPaths) -> Bool {
        let recorded = [journal.sourceDiskPath, journal.sourceVarsPath]
        return recorded == [paths.stagingDisk.path, paths.stagingVars.path]
            || recorded == ["/tmp/bridgevm-appinstall-\(paths.slug)-target.raw",
                            "/tmp/bridgevm-appinstall-\(paths.slug)-vars.fd"]
    }

    /// Digests prove the bytes; ownership proves no other local user supplied the file.
    static func requireOwnedSource(_ url: URL) throws {
        var info = stat()
        guard lstat(url.path, &info) == 0 else {
            throw HvfWindowsInstallFinalizationError.missingArtifact(url.path)
        }
        guard info.st_mode & S_IFMT == S_IFREG, info.st_uid == geteuid() else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
    }
}

extension HvfWindowsInstallFinalizationPaths {
    var staging: URL { HvfWindowsInstallStaging.root(bundle: bundle) }
    var stagingDisk: URL { staging.appendingPathComponent(HvfWindowsInstallStaging.targetName, isDirectory: false) }
    var stagingVars: URL { staging.appendingPathComponent(HvfWindowsInstallStaging.varsName, isDirectory: false) }
    var stagingEvidence: URL { staging.appendingPathComponent(HvfWindowsInstallStaging.evidenceName, isDirectory: true) }
    var stagingLog: URL { stagingEvidence.appendingPathComponent("run.log", isDirectory: false) }
    var bundleInstallLog: URL { bundle.appendingPathComponent("logs/install-run.log", isDirectory: false) }
}

extension HvfWindowsInstallPlan {
    /// The same paths finalization seals, so the journal names exactly what the runner was given.
    private var finalizationPaths: HvfWindowsInstallFinalizationPaths {
        HvfWindowsInstallFinalization.paths(slug: slug, libraryRoot: libraryRoot, bundlePath: bundlePath)
    }
    var stagingDirectory: String { finalizationPaths.staging.path }
    var stagingTargetPath: String { finalizationPaths.stagingDisk.path }
    var stagingVarsPath: String { finalizationPaths.stagingVars.path }
    var stagingEvidenceDir: String { finalizationPaths.stagingEvidence.path }
}

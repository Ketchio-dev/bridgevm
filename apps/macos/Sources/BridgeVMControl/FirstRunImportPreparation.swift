import Foundation

/// Owns the exclusively reserved entry until its configuration is published.
/// This object stays on the import worker; only the value configuration crosses actors.
final class FirstRunImportPreparation {
    let config: VMConfig
    private let destination: FirstRunImportDestination
    private let importedVTPMKey: ImportedVTPMKeyCustody?
    private var preserved = false

    init(config: VMConfig, destination: FirstRunImportDestination, importedVTPMKey: ImportedVTPMKeyCustody?) {
        self.config = config
        self.destination = destination
        self.importedVTPMKey = importedVTPMKey
    }

    func preserve() { preserved = true; importedVTPMKey?.preserve() }
    deinit { if !preserved { destination.removeIfOwned() } }
}

extension FirstRunImport {
    static func prepare(
        _ inputs: Inputs, slug: String, libraryRoot: URL,
        fileManager: FileManager = .default, snapshotHelper: URL = HvfMediaImportHelper.bundled
    ) throws -> FirstRunImportPreparation {
        let destination = try FirstRunImportDestination(
            slug: slug, libraryRoot: libraryRoot, fileManager: fileManager)
        var completed = false
        defer { if !completed { destination.removeIfOwned() } }
        let bundleURL = destination.root.appendingPathComponent("bundle", isDirectory: true)
        let layout = BundleLayout(bundleURL: bundleURL)
        try fileManager.createDirectory(
            at: layout.diskURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(
            at: layout.varsURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(at: layout.vtpmURL, withIntermediateDirectories: true)

        try HvfMediaImport.copy(disk: inputs.diskPath, vars: inputs.varsPath,
            toDisk: layout.diskURL, toVars: layout.varsURL, helper: snapshotHelper, fileManager: fileManager)
        let keyCustody = try FirstRunImportVTPMRecovery.materialize(inputs, destinationStableVMID: slug,
            destination: layout.vtpmURL, fileManager: fileManager)

        var config = VMConfig(
            id: slug,
            name: inputs.displayName,
            displayName: inputs.displayName,
            backendKind: BackendKind.hvfEngine.rawValue,
            bundlePath: bundleURL.path,
            runnerPath: "",
            launchSpecPath: "",
            handoffPath: "",
            sshKeyPath: "",
            sshUser: "bridge",
            leasesPath: "",
            guestName: inputs.displayName,
            displayWidth: 1280,
            displayHeight: 720
        )
        config.diskPath = layout.diskURL.path
        config.memMiB = inputs.memMiB
        config.cpuCount = inputs.cpuCount
        completed = true
        return FirstRunImportPreparation(config: config, destination: destination, importedVTPMKey: keyCustody)
    }
}

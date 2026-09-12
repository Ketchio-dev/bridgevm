import Foundation

// Failed rollback must not claim the absent original bundle is restored.
extension VMLibrary {
    static func moveWindowsHVFBundle(
        _ config: VMConfig,
        to destinationParent: URL,
        rootURL: URL = root,
        fileManager: FileManager = .default
    ) -> VMConfig? {
        guard config.engineKind == .hvfEngine else { return nil }
        let fm = fileManager
        let source = URL(fileURLWithPath: config.bundlePath, isDirectory: true).standardizedFileURL
        let destination = destinationParent.standardizedFileURL
            .appendingPathComponent(source.lastPathComponent, isDirectory: true)
        guard VMRelocationPaths.isSafe(source: source, destination: destination),
              (try? source.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]).isDirectory) == true,
              (try? source.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) != true,
              !fm.fileExists(atPath: destination.path) else { return nil }
        do {
            try fm.createDirectory(at: destinationParent, withIntermediateDirectories: true)
            try fm.moveItem(at: source, to: destination)
        } catch { return nil }

        func rebased(_ path: String) -> String {
            guard !path.isEmpty else { return path }
            if path == source.path { return destination.path }
            let prefix = source.path + "/"
            guard path.hasPrefix(prefix) else { return path }
            return destination.path + "/" + path.dropFirst(prefix.count)
        }

        var moved = config
        moved.bundlePath = destination.path
        moved.runnerPath = rebased(config.runnerPath)
        moved.launchSpecPath = rebased(config.launchSpecPath)
        moved.handoffPath = rebased(config.handoffPath)
        moved.sshKeyPath = rebased(config.sshKeyPath)
        moved.leasesPath = rebased(config.leasesPath)
        moved.isoPath = config.isoPath.map(rebased)
        moved.diskPath = config.diskPath.map(rebased)

        let stateDirectory = destination.appendingPathComponent("metadata/vtpm", isDirectory: true)
        do {
            guard save(moved, rootURL: rootURL) else { throw CocoaError(.fileWriteUnknown) }
            _ = try VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
                .recordSameIdentityMove(
                    stableVMID: moved.slug,
                    stateDirectory: stateDirectory,
                    sourceBundle: source,
                    destinationBundle: destination
                )
            return moved
        } catch {
            try? fm.moveItem(at: destination, to: source)
            if let observed = VMRelocationRecovery.configuration(original: config, moved: moved, fileManager: fm) {
                _ = save(observed, rootURL: rootURL)
            }
            return nil
        }
    }
}

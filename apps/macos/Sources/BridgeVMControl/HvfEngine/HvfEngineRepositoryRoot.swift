import Foundation

extension HvfEngineSession {
    nonisolated static func defaultRepoRoot(
        currentDirectoryPath: String = FileManager.default.currentDirectoryPath,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        executablePath: String? = Bundle.main.executableURL?.path,
        resourcePath: String? = Bundle.main.resourceURL?.path,
        allowDevelopmentOverrides: Bool = _isDebugAssertConfiguration()
    ) -> URL {
        #if DEBUG
        if allowDevelopmentOverrides {
            if let override = environment["BRIDGEVM_REPO_ROOT"]?.trimmingCharacters(in: .whitespacesAndNewlines),
               !override.isEmpty {
                let expanded = (override as NSString).expandingTildeInPath
                let url = URL(fileURLWithPath: expanded, isDirectory: true)
                if containsBootWrapper(url) { return url.resolvingSymlinksInPath() }
            }
            var candidates = [URL(fileURLWithPath: currentDirectoryPath, isDirectory: true)]
            if let executablePath { candidates.append(URL(fileURLWithPath: executablePath).deletingLastPathComponent()) }
            if let resourcePath { candidates.append(URL(fileURLWithPath: resourcePath, isDirectory: true)) }
            for candidate in candidates {
                if let root = repositoryRoot(startingAt: candidate) { return root }
            }
        }
        #endif
        if let resourcePath {
            return URL(fileURLWithPath: resourcePath, isDirectory: true).standardizedFileURL
        }
        return URL(fileURLWithPath: "/BridgeVMUnavailableResources", isDirectory: true)
    }

    private nonisolated static func containsBootWrapper(_ root: URL) -> Bool {
        FileManager.default.isExecutableFile(
            atPath: root.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path
        )
    }

    private nonisolated static func repositoryRoot(startingAt start: URL) -> URL? {
        var candidate = start.standardizedFileURL
        while true {
            if containsBootWrapper(candidate) { return candidate.resolvingSymlinksInPath() }
            let parent = candidate.deletingLastPathComponent()
            if parent.path == candidate.path { return nil }
            candidate = parent
        }
    }

}

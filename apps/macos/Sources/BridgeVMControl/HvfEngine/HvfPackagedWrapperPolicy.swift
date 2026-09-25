import Foundation

enum HvfPackagedWrapperPolicy {
    static func wrapperAvailable(repoRoot: URL, fileManager: FileManager = .default) -> Bool {
        let wrapper = repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        guard fileManager.isExecutableFile(atPath: wrapper.path) else { return false }
        #if !DEBUG
        guard repoRoot.path.hasSuffix(".app/Contents/Resources") else { return false }
        #endif
        guard repoRoot.path.hasSuffix(".app/Contents/Resources") else { return true }
        let cli = repoRoot.appendingPathComponent("target/release/bridgevm")
        return fileManager.isExecutableFile(atPath: cli.path)
    }

    static func environment(_ inherited: [String: String]) -> [String: String] {
        inherited.filter { !$0.key.hasPrefix("BRIDGEVM_") && $0.key != "CARGO_TARGET_DIR" }
    }

    static func signatureVerified(repoRoot: URL) -> Bool {
        #if DEBUG
        guard repoRoot.path.hasSuffix(".app/Contents/Resources") else { return true }
        #endif
        let app = Bundle.main.bundleURL
        guard app.pathExtension == "app",
              repoRoot.resolvingSymlinksInPath().path == app.appendingPathComponent("Contents/Resources").resolvingSymlinksInPath().path else { return false }
        return codeSignatureVerified(app: app)
    }

    static func codeSignatureVerified(app: URL) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/codesign")
        process.arguments = ["--verify", "--deep", "--strict", app.path]
        process.environment = [:]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch { return false }
    }
}

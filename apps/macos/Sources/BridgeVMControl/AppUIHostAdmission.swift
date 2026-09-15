#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import Darwin

@MainActor
extension AppUIHost {
    static func outputDirectory(arguments: [String], fileManager: FileManager = .default) throws -> URL {
        guard arguments.count == 3, arguments[0] == "--app-ui-host", arguments[1] == "--output" else {
            throw AppUIHostError.refused("Expected exactly --app-ui-host --output ABSOLUTE_DIRECTORY")
        }
        let path = arguments[2]
        let components = (path as NSString).pathComponents
        let output = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
        guard path.hasPrefix("/"), path.utf8.count <= 4096, !path.contains("\0"),
              !components.contains(".."), !components.contains("."), output.path == path,
              output.resolvingSymlinksInPath().path == path, output.lastPathComponent == "host-observations",
              output.deletingLastPathComponent().lastPathComponent == "app-ui-private" else {
            throw AppUIHostError.refused("Output must be a canonical app-ui-private/host-observations directory")
        }
        for directory in [output.deletingLastPathComponent(), output] {
            let attributes = try fileManager.attributesOfItem(atPath: directory.path)
            guard attributes[.type] as? FileAttributeType == .typeDirectory,
                  (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid(),
                  (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
                throw AppUIHostError.refused("Output and parent require owned 0700 directories")
            }
        }
        guard try fileManager.contentsOfDirectory(atPath: path).isEmpty else {
            throw AppUIHostError.refused("Output directory must be empty")
        }
        return output
    }
    static func writeIdentity(_ capture: AppUIHostCapture) throws {
        let bundle = Bundle.main.bundleURL.standardizedFileURL
        guard Bundle.main.bundleIdentifier == "dev.bridgevm.app-ui-host",
              bundle.lastPathComponent == "BridgeVMAppUIHost.app", bundle.resolvingSymlinksInPath() == bundle,
              let executable = Bundle.main.executableURL?.standardizedFileURL,
              executable == bundle.appendingPathComponent("Contents/MacOS/BridgeVMControl"),
              executable.resolvingSymlinksInPath() == executable,
              (try executable.resourceValues(forKeys: [.isRegularFileKey])).isRegularFile == true else {
            throw AppUIHostError.refused("Host bundle/executable identity differs from the fixed diagnostic contract")
        }
        try capture.write(["schema_version": 1, "kind": "native-app-ui-host-identity", "pid": Int(getpid()),
            "bundle_identifier": "dev.bridgevm.app-ui-host", "bundle_path": bundle.path,
            "executable_path": executable.path, "executable_sha256": try AppUIHostCapture.digest(executable),
            "started_uptime": ProcessInfo.processInfo.systemUptime], name: "host-identity.json", final: true)
    }
}
#endif

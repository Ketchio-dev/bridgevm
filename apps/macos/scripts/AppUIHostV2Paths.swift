import AppKit
import Foundation
import Darwin

struct AppUIHostV2Paths {
    let root: URL
    let hostSHA256: String
    let launcherSHA256: String
    var observations: URL { root.appendingPathComponent("host-observations") }
    var receipt: URL { root.appendingPathComponent("launcher-observations-v2.json") }
    var cancellation: URL { root.appendingPathComponent("cancel.requested") }
    func bundle(_ role: AppUIHostV2Role) -> URL { root.appendingPathComponent(role.bundleName) }
    func digest(_ role: AppUIHostV2Role) -> String { role == .host ? hostSHA256 : launcherSHA256 }

    init(arguments: [String]) throws {
        guard arguments.count == 7, arguments[0] == "--app-ui-supervisor-v2",
              arguments[1] == "--private-root", arguments[3] == "--host-sha256",
              arguments[5] == "--launcher-sha256" else { throw AppUIDriverFailure.invalidRequest }
        root = URL(fileURLWithPath: arguments[2], isDirectory: true)
        hostSHA256 = arguments[4]; launcherSHA256 = arguments[6]
        guard AppUIDriverValidation.canonicalPath(arguments[2]), root.path == arguments[2],
              root.path.utf8.count <= 4096, !root.path.contains("\0"),
              !(root.path as NSString).pathComponents.contains(".."),
              !(root.path as NSString).pathComponents.contains("."),
              root.resolvingSymlinksInPath().path == root.path, root.lastPathComponent == "app-ui-private",
              [hostSHA256, launcherSHA256].allSatisfy({ $0.count == 64 && $0.allSatisfy { "0123456789abcdef".contains($0) } }) else {
            throw AppUIDriverFailure.invalidRequest
        }
        for directory in [root, observations] {
            let attributes = try FileManager.default.attributesOfItem(atPath: directory.path)
            guard directory.resolvingSymlinksInPath().path == directory.path,
                  attributes[.type] as? FileAttributeType == .typeDirectory,
                  (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid(),
                  (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
                throw AppUIDriverFailure.invalidFile
            }
        }
        guard try FileManager.default.contentsOfDirectory(atPath: observations.path).isEmpty,
              !AppUIHostLaunchPaths.exists(receipt.path) else { throw AppUIDriverFailure.fileExists }
        for role in AppUIHostV2Role.allCases {
            let app = bundle(role)
            let executable = app.appendingPathComponent("Contents/MacOS/" + role.executableName)
            guard app.resolvingSymlinksInPath().path == app.path, let info = Bundle(url: app),
                  info.bundleIdentifier == role.identifier,
                  info.executableURL?.resolvingSymlinksInPath().path == executable.path,
                  try AppUIHostLaunchPaths.digest(executable) == digest(role) else {
                throw AppUIDriverFailure.identityMismatch
            }
        }
    }
}

import Foundation
import Darwin

enum AppUIHostV2PathValidation {
    static func validate(_ paths: AppUIHostV2Paths) throws {
        for directory in [paths.root, paths.observations] {
            let attributes = try FileManager.default.attributesOfItem(atPath: directory.path)
            guard directory.resolvingSymlinksInPath().path == directory.path,
                  attributes[.type] as? FileAttributeType == .typeDirectory,
                  (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid(),
                  (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
                throw AppUIDriverFailure.invalidFile
            }
        }
        guard try FileManager.default.contentsOfDirectory(atPath: paths.observations.path).isEmpty,
              !AppUIHostLaunchPaths.exists(paths.receipt.path) else { throw AppUIDriverFailure.fileExists }
        for role in AppUIHostV2Role.allCases {
            let app = paths.bundle(role)
            let executable = app.appendingPathComponent("Contents/MacOS/" + role.executableName)
            guard app.resolvingSymlinksInPath().path == app.path, let info = Bundle(url: app),
                  info.bundleIdentifier == role.identifier,
                  info.executableURL?.resolvingSymlinksInPath().path == executable.path,
                  try AppUIHostLaunchPaths.digest(executable) == paths.digest(role) else {
                throw AppUIDriverFailure.identityMismatch
            }
        }
    }
}

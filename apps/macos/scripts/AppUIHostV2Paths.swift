import AppKit
import Foundation
import Darwin

struct AppUIHostV2Paths {
    let mode: AppUIHostV2Mode
    let root: URL
    let hostSHA256: String
    let launcherSHA256: String
    var observations: URL { root.appendingPathComponent("host-observations") }
    var receipt: URL { root.appendingPathComponent(mode.receiptName) }
    var cancellation: URL { root.appendingPathComponent("cancel.requested") }
    func bundle(_ role: AppUIHostV2Role) -> URL { root.appendingPathComponent(role.bundleName) }
    func digest(_ role: AppUIHostV2Role) -> String { role == .host ? hostSHA256 : launcherSHA256 }

    init(arguments: [String]) throws {
        guard arguments.count == 7,
              arguments[1] == "--private-root", arguments[3] == "--host-sha256",
              arguments[5] == "--launcher-sha256" else { throw AppUIDriverFailure.invalidRequest }
        mode = try AppUIHostV2Mode(flag: arguments[0])
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
        try AppUIHostV2PathValidation.validate(self)
    }
}

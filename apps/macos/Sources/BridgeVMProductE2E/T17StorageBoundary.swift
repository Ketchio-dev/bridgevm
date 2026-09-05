import Foundation

enum T17StorageBoundary {
    static func validate(_ path: String) throws {
        if path.hasPrefix("/tmp/bridgevm-e2e-") || path.hasPrefix("/private/tmp/bridgevm-e2e-") { return }
        let uuid = "[0-9A-F]{8}(?:-[0-9A-F]{4}){3}-[0-9A-F]{12}"
        let pattern = "^/Volumes/[^/\\s]+/BridgeVM/live-t17/(\(uuid))/bridgevm-e2e-[A-Za-z0-9][A-Za-z0-9._-]{0,127}\\.[A-Za-z0-9]{6}/lane-[1-3]$"
        guard path.range(of: pattern, options: .regularExpression) != nil else {
            throw T17Blocker(code: "invalid-request", detail: "lane is outside pinned external storage")
        }
        let root = URL(fileURLWithPath: path)
        guard root.resolvingSymlinksInPath().path == path else {
            throw T17Blocker(code: "invalid-request", detail: "external lane traverses a symlink")
        }
        let values = try root.resourceValues(forKeys: [.volumeUUIDStringKey, .volumeSupportsFileCloningKey, .volumeIsInternalKey, .volumeIsReadOnlyKey])
        let expected = root.pathComponents[5]
        guard values.volumeUUIDString?.uppercased() == expected,
              values.volumeSupportsFileCloning == true, values.volumeIsInternal == false,
              values.volumeIsReadOnly == false else {
            throw T17Blocker(code: "invalid-request", detail: "external volume changed or cannot clone")
        }
    }
}

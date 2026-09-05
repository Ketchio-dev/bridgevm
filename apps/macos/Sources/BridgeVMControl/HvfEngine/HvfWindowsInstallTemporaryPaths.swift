import Foundation

enum HvfWindowsInstallTemporaryPaths {
    static func prefix(libraryRoot: URL, slug: String) -> String {
        let parent = libraryRoot.path.hasPrefix("/Volumes/")
            ? libraryRoot.deletingLastPathComponent().path : "/tmp"
        return "\(parent)/bridgevm-appinstall-\(slug)"
    }

    static func validationError(libraryRoot: URL) -> String? {
        guard libraryRoot.path.hasPrefix("/Volumes/") else { return nil }
        let lane = libraryRoot.deletingLastPathComponent()
        let uuid = "[0-9A-F]{8}(?:-[0-9A-F]{4}){3}-[0-9A-F]{12}"
        let pattern = "^/Volumes/[^/\\s]+/BridgeVM/live-t17/(\(uuid))/bridgevm-e2e-[A-Za-z0-9][A-Za-z0-9._-]{0,127}\\.[A-Za-z0-9]{6}/lane-[1-3]$"
        guard libraryRoot.lastPathComponent == "library",
              lane.path.range(of: pattern, options: .regularExpression) != nil,
              lane.resolvingSymlinksInPath().path == lane.path,
              let volume = try? lane.resourceValues(forKeys: [.volumeUUIDStringKey, .volumeSupportsFileCloningKey, .volumeIsInternalKey, .volumeIsReadOnlyKey]),
              volume.volumeUUIDString?.uppercased() == lane.pathComponents[5],
              volume.volumeSupportsFileCloning == true, volume.volumeIsInternal == false,
              volume.volumeIsReadOnly == false else {
            return "외장 설치 볼륨의 UUID, APFS 복제 지원 또는 경로를 확인할 수 없습니다."
        }
        return nil
    }
}

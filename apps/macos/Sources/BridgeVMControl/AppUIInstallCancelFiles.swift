#if DEBUG && BRIDGEVM_APP_UI_HOST
import Darwin
import Foundation

struct AppUIInstallCancelFile: Codable, Equatable {
    let path: String
    let bytes: Int
    let sha256: String
}

enum AppUIInstallCancelFiles {
    static let names = ["pending/vm.json", "pending/bundle/metadata/hvf-install.json",
        "pending/bundle/repo/helpers/wimlib-imagex", "pending/bundle/repo/helpers/bridgevm-catalog-verify"]

    static func snapshot(_ root: URL) throws -> [AppUIInstallCancelFile] {
        guard root.resolvingSymlinksInPath().path == root.path else { throw AppUIDriverFailure.invalidFile }
        let entries = try FileManager.default.subpathsOfDirectory(atPath: root.path)
        guard entries.count <= 16 else { throw AppUIDriverFailure.protocolLimit }
        let directories: Set<String> = ["pending", "pending/bundle", "pending/bundle/metadata",
            "pending/bundle/repo", "pending/bundle/repo/helpers"]
        for name in entries {
            var info = stat()
            guard lstat(root.appendingPathComponent(name).path, &info) == 0, info.st_uid == geteuid(),
                  (directories.contains(name) && info.st_mode & S_IFMT == S_IFDIR)
                    || (names.contains(name) && info.st_mode & S_IFMT == S_IFREG && info.st_nlink == 1) else {
                throw AppUIDriverFailure.invalidFile
            }
        }
        guard Set(entries) == directories.union(names) else { throw AppUIDriverFailure.invalidFile }
        return try names.sorted().map { name in
            let data = try HvfWindowsInstallDurability.readRegularFile(root.appendingPathComponent(name), maximumBytes: 16_384)
            return AppUIInstallCancelFile(path: name, bytes: data.count, sha256: AppUIDriverCanonicalJSON.digest(data))
        }
    }
}
#endif

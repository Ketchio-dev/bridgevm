import Foundation

extension HvfWindowsInstallCacheIdentity {
    static func sha256File(_ path: String) -> String? {
        try? HvfWindowsStableFileDigest.compute(path)
    }
}

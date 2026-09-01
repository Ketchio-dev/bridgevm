import Foundation

extension HvfWindowsBootSeed {
    /// Materialize only the bundle-owned seed; release installs never accept an ambient varstore.
    static func writeBundledSeed(to path: String) throws {
        try bundledSeed().write(to: URL(fileURLWithPath: path), options: [.atomic])
    }
}

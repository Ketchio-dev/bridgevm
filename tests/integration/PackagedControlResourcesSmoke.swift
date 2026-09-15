import CryptoKit
import Foundation

// A packaged executable must not evaluate the generated build-path fallback,
// even when its required assets are absent. The real resolver and callers are
// compiled below; this tripwire substitutes only SwiftPM's generated accessor.
extension Bundle {
    static var module: Bundle { fatalError("Packaged resource lookup escaped to the SwiftPM build fallback") }
}

@main
enum PackagedControlResourcesSmoke {
    static func require(_ condition: @autoclosure () -> Bool, _ message: String) throws {
        if !condition() {
            throw NSError(domain: "PackagedControlResourcesSmoke", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: message])
        }
    }
    static func seed() throws {
        let bytes = try HvfWindowsBootSeed.bundledSeed()
        let hash = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        try require(bytes.count == 67_108_864, "bundled seed size changed")
        try require(hash == "c9c026c43869a37fd0237094c255052c821e2ff6a3d887be305d1b7105e2800b",
                    "bundled seed no longer matches its public fixture")
        try require(bytes.range(of: HvfWindowsBootSeed.sentinelGUID) != nil, "seed lost its patchable sentinel")
    }
    static func policy() throws {
        let policy = try HvfSecureBootProvisioner.bundledPolicy()
        try require(policy.policy == "microsoft-windows-transition-2011-2023", "unexpected trusted policy")
        try require(policy.variables.map(\.name) == ["dbx", "db", "KEK", "PK"], "trusted variables changed")
    }
    static func missingSeed() throws {
        do { _ = try HvfWindowsBootSeed.bundledSeed() }
        catch HvfWindowsBootSeed.SeedError.varstoreUnreadable { return }
        throw NSError(domain: "PackagedControlResourcesSmoke", code: 2,
                      userInfo: [NSLocalizedDescriptionKey: "missing packaged seed did not use the existing typed error"])
    }
    static func missingPolicy() throws {
        do { _ = try HvfSecureBootProvisioner.bundledPolicy() }
        catch HvfWindowsBootSeed.SeedError.secureBootManifestMissing { return }
        throw NSError(domain: "PackagedControlResourcesSmoke", code: 3,
                      userInfo: [NSLocalizedDescriptionKey: "missing packaged policy did not use the existing typed error"])
    }
    static func main() throws {
        guard CommandLine.arguments.count == 2 else { exit(64) }
        try require(Bundle.main.bundleURL.pathExtension == "app", "probe did not run from the relocated app layout")
        switch CommandLine.arguments[1] {
        case "present": try seed(); try policy()
        case "missing-seed": try missingSeed(); try policy()
        case "missing-policy": try seed(); try missingPolicy()
        case "missing-bundle", "root-decoy": try missingSeed(); try missingPolicy()
        case "corrupt-policy":
            do {
                _ = try HvfSecureBootProvisioner.bundledPolicy()
                throw NSError(domain: "PackagedControlResourcesSmoke", code: 4,
                              userInfo: [NSLocalizedDescriptionKey: "corrupt owned policy was accepted"])
            } catch HvfWindowsBootSeed.SeedError.secureBootManifestInvalid { }
        default: exit(64)
        }
        print("PASS: relocated packaged resources \(CommandLine.arguments[1]) (no NSApplication or guest)")
    }
}

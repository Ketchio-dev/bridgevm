import Foundation

// Direct swiftc builds do not get SwiftPM's generated Bundle.module accessor.
extension Bundle {
    static var module: Bundle { .main }
}

@main
struct HvfSecureBootProvisioningSmoke {
    static func require(_ condition: @autoclosure () -> Bool, _ message: String) {
        if !condition() {
            FileHandle.standardError.write(Data("FAIL: \(message)\n".utf8))
            exit(1)
        }
    }

    static func emptyVarstore(size: Int = 256 * 1024) -> Data {
        var store = Data(count: size)
        store.replaceSubrange(0x28..<0x2b, with: Data("_FV".utf8))
        store[0x30] = 0x48
        store[0x31] = 0
        let base = 0x48
        store.replaceSubrange(base..<base + 16, with: HvfWindowsBootSeed.authVarStoreGUID)
        let value = UInt32(size - base)
        for index in 0..<4 {
            store[base + 16 + index] = UInt8((value >> (8 * UInt32(index))) & 0xff)
        }
        store[base + 20] = 0x5a
        store[base + 21] = 0xfe
        return store
    }

    static func main() throws {
        guard CommandLine.arguments.count == 2 else { exit(64) }
        let policyData = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
        let policy = try JSONDecoder().decode(HvfSecureBootPolicy.self, from: policyData)
        let decoded = try HvfSecureBootProvisioner.decodedVariables(policy)
        require(decoded.map(\.manifest.name) == ["dbx", "db", "KEK", "PK"], "policy order")

        let original = emptyVarstore()
        let first = try HvfSecureBootProvisioner.provision(
            varStore: original,
            policy: policy,
            provisionedAt: Date(timeIntervalSince1970: 1_700_000_000))
        require(first.varStore.count == original.count, "varstore size changed")
        let keys = try HvfSecureBootProvisioner.storedVariables(in: first.varStore)
            .filter { ["dbx", "db", "KEK", "PK"].contains($0.name) }
            .sorted { $0.offset < $1.offset }
        require(keys.map(\.name) == ["dbx", "db", "KEK", "PK"], "PK was not written last")
        require(keys.allSatisfy { $0.attributes == 0x27 }, "authenticated attributes differ")

        let second = try HvfSecureBootProvisioner.provision(
            varStore: first.varStore,
            policy: policy)
        require(second.varStore == first.varStore, "provisioning is not idempotent")

        let dbx = decoded[0]
        let record = HvfWindowsBootSeed.authVariable(
            name: dbx.manifest.name,
            guid: dbx.guid,
            data: dbx.payload,
            attributes: dbx.manifest.attributes)
        var partial = original
        partial.replaceSubrange(0x60..<0x60 + record.count, with: record)
        do {
            _ = try HvfSecureBootProvisioner.provision(varStore: partial, policy: policy)
            require(false, "partial policy was accepted")
        } catch HvfWindowsBootSeed.SeedError.secureBootConflict {
            // Expected: never complete or overwrite an existing partial policy.
        }

        var malformed = Data(count: 28)
        malformed[16] = 0xff
        do {
            try HvfSecureBootProvisioner.validateSignatureLists(malformed, name: "db")
            require(false, "malformed ESL was accepted")
        } catch HvfWindowsBootSeed.SeedError.secureBootManifestInvalid {
            // Expected.
        }

        print("PASS: pinned Secure Boot policy, PK-last, idempotency, and conflict safety")
    }
}

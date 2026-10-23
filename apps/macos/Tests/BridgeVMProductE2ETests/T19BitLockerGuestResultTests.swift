import XCTest
@testable import BridgeVMProductE2E

/// The guest BitLocker result is parsed exactly and read bounded; any other shape is a fixed reason,
/// so a guest can turn the check into nothing but codes, small integers and PCR hashes.
final class T19BitLockerGuestResultTests: XCTestCase {
    static let nonce = String(repeating: "c3", count: 32)
    static let pcr0 = String(repeating: "0a", count: 32), pcr7 = String(repeating: "7b", count: 32)

    static func body(phase: String = "enable", nonce: String = nonce, outcome: String = "ok", edition: String = "pro",
                     volume: String = "2,0,0,0", protectors: String = "1", profile: String = "7,11", fve: String = "none",
                     read: String = "ok", pcr0: String = pcr0, pcr7: String = pcr7) -> String {
        "bridgevm-t19-bitlocker-v1\nnonce=\(nonce)\nphase=\(phase)\noutcome=\(outcome)\nedition=\(edition)\nvolume=\(volume)\n"
            + "protectors=\(protectors)\npcr_profile=\(profile)\nfve_code=\(fve)\npcr_read=\(read)\npcr0_sha256=\(pcr0)\npcr7_sha256=\(pcr7)\n"
    }

    static let enabled = T19BitLockerGuestState(outcome: nil, edition: "pro", conversion: 2, protection: 0, lock: 0, percent: 0,
                                                protectors: [1], pcrProfile: [7, 11], fveCode: nil, pcrRead: "ok",
                                                pcr0SHA256: pcr0, pcr7SHA256: pcr7)

    private func parse(_ text: String, phase: T19BitLockerPhase = .enable) -> Result<T19BitLockerGuestState, T19BitLockerReason> {
        T19BitLockerGuestResult.parse(Data(text.utf8), nonce: Self.nonce, phase: phase)
    }

    func testAvailableResultsKeepOnlyCodesIntegersAndHashes() {
        XCTAssertEqual(parse(Self.body()), .success(Self.enabled))
        var unlocked = Self.enabled; unlocked.conversion = 1; unlocked.protection = 1; unlocked.percent = 100; unlocked.pcrProfile = [0, 2, 4, 11]
        XCTAssertEqual(parse(Self.body(phase: "status", volume: "1,1,0,100", profile: "0,2,4,11"), phase: .status), .success(unlocked))
    }

    func testGuestRefusalsUnknownsAndCodesAreKept() {
        XCTAssertEqual(T19BitLockerReason.guestReported.map(\.rawValue), [
            "edition-unsupported", "tpm-not-ready", "bitlocker-unavailable", "volume-not-decrypted", "protectors-present",
            "protector-add-failed", "protector-unexpected", "encrypt-failed", "query-failed", "script-failed"])
        for code in T19BitLockerReason.guestReported {
            let result = parse(Self.body(outcome: code.rawValue, edition: "unknown", volume: "unknown", protectors: "unknown",
                                         profile: "unknown", read: "helper-failed", pcr0: "absent", pcr7: "absent"))
            XCTAssertEqual(result, .success(T19BitLockerGuestState(outcome: code, pcrRead: "helper-failed")), code.rawValue)
        }
        var refused = Self.enabled; refused.outcome = .encryptFailed; refused.conversion = 0; refused.fveCode = 65535
        XCTAssertEqual(parse(Self.body(outcome: "encrypt-failed", volume: "0,0,0,0", fve: "65535")), .success(refused))
        var recovery = Self.enabled; recovery.outcome = .volumeNotDecrypted; recovery.conversion = 1; recovery.percent = 100
        recovery.protectors = [3]; recovery.pcrProfile = []
        XCTAssertEqual(parse(Self.body(outcome: "volume-not-decrypted", volume: "1,0,0,100", protectors: "3", profile: "none")), .success(recovery))
    }

    func testEveryOtherShapeIsMalformed() {
        let valid = Self.body()
        var lines = valid.components(separatedBy: "\n"); lines.swapAt(5, 6)
        let cases: [String: String] = [
            "another nonce": Self.body(nonce: String(repeating: "d", count: 64)), "another phase": Self.body(phase: "status"),
            "unknown outcome": Self.body(outcome: "on-fire"), "host reason from the guest": Self.body(outcome: "result-missing"),
            "edition outside the set": Self.body(edition: "ultimate"), "conversion 6": Self.body(volume: "6,0,0,0"),
            "percent 101": Self.body(volume: "2,0,0,101"), "leading zero": Self.body(volume: "2,0,0,07"),
            "three volume fields": Self.body(volume: "2,0,0"), "unsorted list": Self.body(profile: "11,7"),
            "repeated list": Self.body(protectors: "1,1"), "protector type 11": Self.body(protectors: "11"),
            "PCR 24": Self.body(profile: "7,24"), "empty list item": Self.body(profile: "7,,11"),
            "fve 65536": Self.body(fve: "65536"), "negative fve": Self.body(fve: "-1"), "fve with leading zero": Self.body(fve: "048"),
            "unknown TBS code": Self.body(read: "tpm-on-fire", pcr0: "absent", pcr7: "absent"),
            "hash after a failed read": Self.body(read: "tbs-submit-failed"), "absent hash after ok": Self.body(pcr0: "absent"),
            "uppercase hash": Self.body(pcr7: Self.pcr7.uppercased()), "short hash": Self.body(pcr0: String(Self.pcr0.dropLast())),
            "missing final newline": String(valid.dropLast()), "CRLF": valid.replacingOccurrences(of: "\n", with: "\r\n"),
            "extra line": valid + "pcr1_sha256=\(Self.pcr0)\n", "missing line": valid.replacingOccurrences(of: "fve_code=none\n", with: ""),
            "reordered": lines.joined(separator: "\n"), "other version": valid.replacingOccurrences(of: "bitlocker-v1", with: "bitlocker-v2"),
            "empty": "",
        ]
        for (name, text) in cases.sorted(by: { $0.key < $1.key }) {
            XCTAssertEqual(parse(text), .failure(.resultMalformed), name)
        }
        XCTAssertEqual(T19BitLockerGuestResult.parse(Data([0xc3, 0xa9, 0x0a]), nonce: Self.nonce, phase: .enable), .failure(.resultMalformed))
        XCTAssertEqual(parse(valid + String(repeating: "#", count: 512)), .failure(.resultMalformed))
    }

    func testResultFileIsNamedPerPhaseBoundedRegularAndNeverFollowed() throws {
        let share = FileManager.default.temporaryDirectory.appendingPathComponent("t19-bitlocker-result-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: share, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: share) }
        let name = T19BitLockerGuestResult.name(phase: .status, prefix: String(Self.nonce.prefix(12)))
        XCTAssertEqual(name, "t19-bitlocker-status-c3c3c3c3c3c3.txt")
        let url = share.appendingPathComponent(name)
        func read() -> Result<T19BitLockerGuestState, T19BitLockerReason> { T19BitLockerGuestResult.read(url, nonce: Self.nonce, phase: .status) }
        XCTAssertEqual(read(), .failure(.resultMissing))
        try Data(Self.body(phase: "status").utf8).write(to: url)
        XCTAssertEqual(read(), .success(Self.enabled))
        try Data(repeating: 0x61, count: T19BitLockerGuestResult.maximumBytes + 1).write(to: url)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
        try Data().write(to: url)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
        try FileManager.default.removeItem(at: url)
        let target = share.appendingPathComponent("elsewhere.txt")
        try Data(Self.body(phase: "status").utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: url, withDestinationURL: target)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
        try FileManager.default.removeItem(at: url)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
    }

    /// The guest script's own template, rendered as PowerShell would, is what the host accepts, and its
    /// longest legal rendering stays within the bound.
    func testTheGuestScriptWritesWhatTheHostParses() throws {
        let script = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("scripts/win-assets/bv-product-e2e-bitlocker.ps1")
        let source = String(decoding: try Data(contentsOf: script), as: UTF8.self).replacingOccurrences(of: "\r\n", with: "\n")
        let line = try XCTUnwrap(source.components(separatedBy: "\n").first { $0.hasPrefix("    \"bridgevm-t19-bitlocker-v1`n") })
        func render(_ values: [String: String]) -> String {
            var body = String(line.trimmingCharacters(in: .whitespaces).dropFirst().dropLast()).replacingOccurrences(of: "`n", with: "\n")
            while let name = body.range(of: #"\$\(\$[A-Za-z]+\.[A-Za-z0-9]+\)|\$[A-Za-z0-9]+"#, options: .regularExpression) {
                let token = String(body[name]).replacingOccurrences(of: "$(", with: "").replacingOccurrences(of: ")", with: "")
                body.replaceSubrange(name, with: values[token.replacingOccurrences(of: "$", with: "")] ?? "<missing \(token)>")
            }
            return body
        }
        let observed = ["Nonce": Self.nonce, "Phase": "enable", "Result.Outcome": "ok", "Result.Edition": "pro", "Volume": "2,0,0,0",
                        "Result.Protectors": "1", "Result.Profile": "7,11", "Result.Fve": "none", "Pcr.Status": "ok",
                        "Pcr.Pcr0": Self.pcr0, "Pcr.Pcr7": Self.pcr7]
        XCTAssertEqual(parse(render(observed)), .success(Self.enabled))
        let longest = render(observed.merging(["Phase": "status", "Result.Outcome": "bitlocker-unavailable", "Result.Edition": "enterprise",
                                               "Volume": "5,2,1,100", "Result.Protectors": (0...10).map(String.init).joined(separator: ","),
                                               "Result.Profile": (0...23).map(String.init).joined(separator: ","), "Result.Fve": "65535"]) { $1 })
        XCTAssertLessThan(longest.utf8.count, T19BitLockerGuestResult.maximumBytes + 1)
        XCTAssertEqual((try? parse(longest, phase: .status).get())?.protectors?.count, 11)
    }
}

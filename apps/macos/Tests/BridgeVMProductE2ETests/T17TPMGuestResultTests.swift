import XCTest
@testable import BridgeVMProductE2E

/// The guest TPM result is parsed exactly and read bounded; any other shape is a fixed
/// reason, so a guest can never turn the evidence into anything but hashes and booleans.
final class T17TPMGuestResultTests: XCTestCase {
    static let nonce = String(repeating: "a", count: 64)
    static let pcr0 = String(repeating: "0a", count: 32), pcr7 = String(repeating: "7b", count: 32)

    static func body(boot: Int = 1, nonce: String = nonce, present: String = "true", ready: String = "true",
                     read: String = "ok", pcr0: String = pcr0, pcr7: String = pcr7) -> String {
        "bridgevm-t17-tpm-pcr-v1\nnonce=\(nonce)\nboot=\(boot)\ntpm_present=\(present)\ntpm_ready=\(ready)\n"
            + "pcr_read=\(read)\npcr0_sha256=\(pcr0)\npcr7_sha256=\(pcr7)\n"
    }

    private func parse(_ text: String, boot: T17TPMBoot = .first) -> Result<T17TPMBootRecord, T17TPMReason> {
        T17TPMGuestResult.parse(Data(text.utf8), nonce: Self.nonce, boot: boot)
    }

    func testAvailableResultKeepsOnlyBooleansAndHashes() {
        XCTAssertEqual(parse(Self.body()), .success(T17TPMBootRecord(
            boot: .first, reason: nil, tpmPresent: true, tpmReady: true, pcr0SHA256: Self.pcr0, pcr7SHA256: Self.pcr7)))
        XCTAssertEqual(parse(Self.body(boot: 3, present: "false", ready: "false"), boot: .final), .success(T17TPMBootRecord(
            boot: .final, reason: nil, tpmPresent: false, tpmReady: false, pcr0SHA256: Self.pcr0, pcr7SHA256: Self.pcr7)))
    }

    func testGuestFailuresBecomeFixedReasonsAndKeepWhatWasObserved() {
        XCTAssertEqual(T17TPMReason.guestReported.map(\.rawValue), [
            "tbs-context-failed", "tbs-submit-failed", "tpm-error-response", "pcr-selection-mismatch",
            "pcr-response-malformed", "helper-failed"])
        for code in T17TPMReason.guestReported {
            XCTAssertEqual(parse(Self.body(present: "true", ready: "false", read: code.rawValue, pcr0: "absent", pcr7: "absent")),
                           .success(T17TPMBootRecord(boot: .first, reason: code, tpmPresent: true, tpmReady: false)), code.rawValue)
        }
        XCTAssertEqual(parse(Self.body(present: "unknown", ready: "unknown")), .success(T17TPMBootRecord(
            boot: .first, reason: .tpmQueryFailed, pcr0SHA256: Self.pcr0, pcr7SHA256: Self.pcr7)))
        XCTAssertEqual(parse(Self.body(present: "unknown", ready: "unknown", read: "tbs-context-failed", pcr0: "absent", pcr7: "absent")),
                       .success(T17TPMBootRecord(boot: .first, reason: .tbsContextFailed)))
    }

    func testEveryOtherShapeIsMalformed() {
        let valid = Self.body()
        var lines = valid.components(separatedBy: "\n"); lines.swapAt(3, 4)
        let cases: [String: String] = [
            "another nonce": Self.body(nonce: String(repeating: "b", count: 64)),
            "another boot": Self.body(boot: 2),
            "unknown code": Self.body(read: "tpm-on-fire", pcr0: "absent", pcr7: "absent"),
            "host reason from the guest": Self.body(read: "result-missing", pcr0: "absent", pcr7: "absent"),
            "uppercase hash": Self.body(pcr0: Self.pcr0.uppercased()),
            "short hash": Self.body(pcr7: String(Self.pcr7.dropLast())),
            "hash after a failed read": Self.body(read: "tbs-submit-failed"),
            "absent hash after ok": Self.body(pcr0: "absent"),
            "one absent hash": Self.body(read: "helper-failed", pcr0: "absent"),
            "boolean spelling": Self.body(present: "True"),
            "missing final newline": String(valid.dropLast()),
            "CRLF": valid.replacingOccurrences(of: "\n", with: "\r\n"),
            "extra line": valid + "pcr1_sha256=\(Self.pcr0)\n",
            "missing line": valid.replacingOccurrences(of: "pcr7_sha256=\(Self.pcr7)\n", with: ""),
            "reordered": lines.joined(separator: "\n"),
            "other version": valid.replacingOccurrences(of: "pcr-v1", with: "pcr-v2"),
            "empty": "",
        ]
        for (name, text) in cases.sorted(by: { $0.key < $1.key }) {
            XCTAssertEqual(parse(text), .failure(.resultMalformed), name)
        }
        XCTAssertEqual(T17TPMGuestResult.parse(Data([0xff, 0x0a]), nonce: Self.nonce, boot: .first), .failure(.resultMalformed))
        XCTAssertEqual(T17TPMGuestResult.parse(Data((valid + String(repeating: "#", count: 512)).utf8), nonce: Self.nonce, boot: .first),
                       .failure(.resultMalformed))
    }

    func testResultFileIsNamedPerBootBoundedRegularAndNeverFollowed() throws {
        let share = FileManager.default.temporaryDirectory.appendingPathComponent("t17-tpm-result-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: share, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: share) }
        let name = T17TPMGuestResult.name(boot: .mutation, prefix: String(Self.nonce.prefix(12)))
        XCTAssertEqual(name, "t17-tpm-boot2-aaaaaaaaaaaa.txt")
        let url = share.appendingPathComponent(name)
        func read() -> Result<T17TPMBootRecord, T17TPMReason> { T17TPMGuestResult.read(url, nonce: Self.nonce, boot: .mutation) }
        XCTAssertEqual(read(), .failure(.resultMissing))
        try Data(Self.body(boot: 2).utf8).write(to: url)
        XCTAssertEqual(read(), .success(T17TPMBootRecord(
            boot: .mutation, reason: nil, tpmPresent: true, tpmReady: true, pcr0SHA256: Self.pcr0, pcr7SHA256: Self.pcr7)))
        try Data(repeating: 0x61, count: T17TPMGuestResult.maximumBytes + 1).write(to: url)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
        try Data().write(to: url)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
        try FileManager.default.removeItem(at: url)
        let target = share.appendingPathComponent("elsewhere.txt")
        try Data(Self.body(boot: 2).utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: url, withDestinationURL: target)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
        try FileManager.default.removeItem(at: url)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        XCTAssertEqual(read(), .failure(.resultUnsafe))
    }

    /// The guest script's own template, rendered as PowerShell would (whole variable names), is what the host accepts.
    func testTheGuestScriptWritesWhatTheHostParses() throws {
        let script = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("scripts/win-assets/bv-product-e2e-tpm.ps1")
        let source = String(decoding: try Data(contentsOf: script), as: UTF8.self).replacingOccurrences(of: "\r\n", with: "\n")
        let template = try XCTUnwrap(source.components(separatedBy: "\n").first { $0.hasPrefix("$Body = \"") })
        func render(_ values: [String: String]) -> String {
            var body = String(template.dropFirst("$Body = \"".count).dropLast()).replacingOccurrences(of: "`n", with: "\n")
            while let name = body.range(of: #"\$[A-Za-z0-9]+"#, options: .regularExpression) { body.replaceSubrange(name, with: values[String(body[name].dropFirst())] ?? "<missing>") }
            return body
        }
        let observed = ["Nonce": Self.nonce, "Boot": "3", "Present": "true", "Ready": "false", "Read": "ok",
                        "Pcr0": Self.pcr0, "Pcr7": Self.pcr7]
        XCTAssertEqual(parse(render(observed), boot: .final), .success(T17TPMBootRecord(
            boot: .final, reason: nil, tpmPresent: true, tpmReady: false, pcr0SHA256: Self.pcr0, pcr7SHA256: Self.pcr7)))
        let longest = render(["Nonce": Self.nonce, "Boot": "1", "Present": "unknown", "Ready": "unknown",
                              "Read": "pcr-response-malformed", "Pcr0": "absent", "Pcr7": "absent"])
        XCTAssertEqual(parse(longest), .success(T17TPMBootRecord(boot: .first, reason: .pcrResponseMalformed)))
        XCTAssertLessThan(render(observed.merging(["Present": "unknown", "Ready": "unknown"]) { $1 }).utf8.count,
                          T17TPMGuestResult.maximumBytes)
    }
}

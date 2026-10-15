import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIOutputTests: XCTestCase {
    private let payload = "개발\u{1b}[2J\u{1b}[HFAKE STATUS\nRuntime state: running\u{1b}]0;title\u{7}\r\t\u{85}\u{2028}\u{2029}\u{202e}\u{2067}"
    private let escaped = #"개발\u001b[2J\u001b[HFAKE STATUS\nRuntime state: running\u001b]0;title\u0007\r\t\u0085\u2028\u2029\u202e\u2067"#

    private func record(overrides: [String: Any] = [:]) throws -> NativeLibraryRecord {
        let fields: [String: Any] = [
            "id": "개발-vm", "displayName": "개발 Windows 👩‍💻", "backendKind": "hvf-engine",
            "cpuCount": 4, "memoryMiB": 6144, "installPending": false,
            "configPath": "/VM library/개발-vm/vm.json", "bundlePath": "/VM library/개발 bundle.vmbridge",
            "runtimeState": "unobserved", "recoveryObservations": ["relocation-record-present-or-unreadable"]
        ]
        return try JSONDecoder().decode(NativeLibraryRecord.self, from: JSONSerialization.data(
            withJSONObject: fields.merging(overrides) { _, new in new }))
    }

    private func inventory(_ value: String) throws -> NativeLibrarySnapshot {
        var fields = Dictionary(uniqueKeysWithValues:
            ["id", "displayName", "backendKind", "configPath", "bundlePath"].map { ($0, value as Any) })
        fields["recoveryObservations"] = [value]
        return NativeLibrarySnapshot(libraryPath: value, records: [try record(overrides: fields)],
            issues: [NativeLibraryIssue(code: value, path: value, message: value)])
    }

    private func readiness(_ value: String) -> NativeCLIReadiness {
        NativeCLIReadiness(libraryPath: value, id: value, backendKind: value,
            engineChecksPerformed: true, launchReady: false,
            launchBlockers: [.init(HvfWindowsReadinessIssue(code: value, scope: .launch, summary: value))],
            releaseBlockers: [.init(HvfWindowsReadinessIssue(code: value, scope: .release, summary: value))],
            productLimitations: [value], recoveryObservations: [value])
    }

    private func assertLayout(_ text: String, lines: Int, tabs: Int,
                              file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(text.filter { $0 == "\n" }.count, lines, file: file, line: line)
        XCTAssertEqual(text.filter { $0 == "\t" }.count, tabs, file: file, line: line)
        for scalar in text.unicodeScalars where scalar != "\n" && scalar != "\t" {
            let code = scalar.value
            XCTAssertFalse(code < 0x20 || (0x7f...0x9f).contains(code) || code == 0x061c
                || (0x200e...0x200f).contains(code) || (0x2028...0x202e).contains(code)
                || (0x2066...0x206f).contains(code), "Active control: \(code)", file: file, line: line)
        }
        XCTAssertFalse(text.components(separatedBy: "\n").contains("Runtime state: running"),
                       file: file, line: line)
    }

    func testOrdinaryInventoryKeepsPathsUnicodeQuotesAndLayout() throws {
        let snapshot = NativeLibrarySnapshot(libraryPath: "/VM library/개발", records: [try record()], issues: [])
        XCTAssertEqual(NativeCLI.render(snapshot), """
        Native VM library: /VM library/개발
        Runtime state: unobserved
        개발-vm\t"개발 Windows 👩‍💻"\thvf-engine
          Configuration: /VM library/개발-vm/vm.json
          Bundle: /VM library/개발 bundle.vmbridge
          Saved resources: 4 CPU, 6144 MiB
          Installation pending: false
          Recovery: relocation-record-present-or-unreadable

        """)
        XCTAssertEqual(NativeCLI.render(NativeLibrarySnapshot(libraryPath: "/VM library", records: [], issues: [])),
            "Native VM library: /VM library\nRuntime state: unobserved\nNo readable saved VMs.\n")
    }

    func testDisplayNameKeepsExistingJSONQuotesWithoutDoubleEscaping() throws {
        let record = try record(overrides: ["displayName": "개발 \"Windows\" \\ VM\n"])
        let snapshot = NativeLibrarySnapshot(libraryPath: "/VM library", records: [record], issues: [])
        let row = NativeCLI.render(snapshot).components(separatedBy: "\n")[2]
        XCTAssertEqual(row, "개발-vm\t" + #""개발 \"Windows\" \\ VM\n""# + "\thvf-engine")
    }

    func testInventoryEscapesEveryExternalTextFieldWithoutForgedLines() throws {
        let text = NativeCLI.render(try inventory(payload))
        XCTAssertEqual(text, """
        Native VM library: \(escaped)
        Runtime state: unobserved
        \(escaped)\t"\(escaped)"\t\(escaped)
          Configuration: \(escaped)
          Bundle: \(escaped)
          Saved resources: 4 CPU, 6144 MiB
          Installation pending: false
          Recovery: \(escaped)
        Issue [\(escaped)]: \(escaped): \(escaped)

        """)
        assertLayout(text, lines: 9, tabs: 2)
    }

    func testOrdinaryReadinessKeepsTextAndStaticLayout() {
        XCTAssertEqual(NativeCLI.render(readiness("개발 Windows 👩‍💻")), """
        Native VM readiness: 개발 Windows 👩‍💻
        Runtime state: unobserved
        Launch prerequisites: blocked
        Launch blocker [개발 Windows 👩‍💻]: 개발 Windows 👩‍💻
        Product release gate [개발 Windows 👩‍💻]: 개발 Windows 👩‍💻
        Limitation: 개발 Windows 👩‍💻
        Recovery: 개발 Windows 👩‍💻
        Prerequisite query only; no VM boot or product release result was observed.

        """)
    }

    func testReadinessEscapesIDsIssuesRecoveryAndLimitations() {
        let text = NativeCLI.render(readiness(payload))
        XCTAssertEqual(text, """
        Native VM readiness: \(escaped)
        Runtime state: unobserved
        Launch prerequisites: blocked
        Launch blocker [\(escaped)]: \(escaped)
        Product release gate [\(escaped)]: \(escaped)
        Limitation: \(escaped)
        Recovery: \(escaped)
        Prerequisite query only; no VM boot or product release result was observed.

        """)
        assertLayout(text, lines: 8, tabs: 0)
    }

    func testInventoryJSONPreservesOriginalValuesAcrossTextRendering() throws {
        let snapshot = try inventory(payload)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        let before = try encoder.encode(snapshot)
        _ = NativeCLI.render(snapshot)
        XCTAssertEqual(try encoder.encode(snapshot), before)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: before) as? [String: Any])
        XCTAssertEqual(object["libraryPath"] as? String, payload)
        let records = try XCTUnwrap(object["records"] as? [[String: Any]])
        for key in ["id", "displayName", "backendKind", "configPath", "bundlePath"] {
            XCTAssertEqual(records.first?[key] as? String, payload, key)
        }
        XCTAssertEqual(records.first?["recoveryObservations"] as? [String], [payload])
        let issues = try XCTUnwrap(object["issues"] as? [[String: String]])
        XCTAssertEqual(issues, [["code": payload, "path": payload, "message": payload]])
    }

    func testReadinessJSONPreservesOriginalValuesAcrossTextRendering() throws {
        let snapshot = readiness(payload)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        let before = try encoder.encode(snapshot)
        _ = NativeCLI.render(snapshot)
        XCTAssertEqual(try encoder.encode(snapshot), before)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: before) as? [String: Any])
        for key in ["id", "backendKind", "libraryPath"] { XCTAssertEqual(object[key] as? String, payload, key) }
        for key in ["launchBlockers", "releaseBlockers"] {
            let issues = try XCTUnwrap(object[key] as? [[String: String]])
            XCTAssertEqual(issues.first?["code"], payload, key)
            XCTAssertEqual(issues.first?["summary"], payload, key)
        }
        for key in ["productLimitations", "recoveryObservations"] {
            XCTAssertEqual(object[key] as? [String], [payload], key)
        }
    }
}

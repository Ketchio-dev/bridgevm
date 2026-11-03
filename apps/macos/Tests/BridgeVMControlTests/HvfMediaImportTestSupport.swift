import Foundation
import XCTest

enum HvfMediaImportTestSupport {
    /// The real helper, provided the way CI provides it: the harness exports the path that
    /// scripts/prepare-hvf-import-test-helper.sh built, and a plain `swift test` runs that
    /// script here once (cargo rebuilds a stale helper). Missing native coverage is a failure.
    static var helper: URL {
        if let url = prepared.url { return url }
        XCTFail(prepared.failure)
        return URL(fileURLWithPath: "/dev/null/snapshot_pair_cli")
    }

    private static let prepared: (url: URL?, failure: String) = {
        if let path = ProcessInfo.processInfo.environment["BRIDGEVM_TEST_SNAPSHOT_HELPER"] {
            return (URL(fileURLWithPath: path).resolvingSymlinksInPath(), "")
        }
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let script = repository.appendingPathComponent("scripts/prepare-hvf-import-test-helper.sh")
        let process = Process(), output = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [script.path]
        process.standardOutput = output  // the helper's path; cargo reports on stderr
        guard (try? process.run()) != nil else { return (nil, "could not run \(script.path)") }
        let path = String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        process.waitUntilExit()
        guard process.terminationStatus == 0, !path.isEmpty else {
            return (nil, "snapshot_pair_cli was not built: \(script.path) exited \(process.terminationStatus)"
                + " (its cargo output is above); fix the build or set BRIDGEVM_TEST_SNAPSHOT_HELPER")
        }
        return (URL(fileURLWithPath: path).resolvingSymlinksInPath(), "")
    }()
}

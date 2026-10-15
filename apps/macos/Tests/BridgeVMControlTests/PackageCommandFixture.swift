import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// Only the owned sudo fixture is discoverable. It records arguments and never
/// executes apt, sudo, SSH or anything named by those arguments.
final class PackageCommandFixture {
    let root: URL
    let bin: URL
    let argumentsFile: URL
    let status: Int32
    private let script: Data
    private var ownsRoot = false

    init(status: Int32) throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        root = URL(fileURLWithPath: temporary).resolvingSymlinksInPath()
            .appendingPathComponent("package outcome ' " + UUID().uuidString)
        bin = root.appendingPathComponent("bin")
        argumentsFile = root.appendingPathComponent("arguments.txt")
        self.status = status
        script = Data("""
        #!/bin/sh
        [ "$1" = 'DEBIAN_FRONTEND=noninteractive' ] || exit 91
        [ "$2" = 'apt-get' ] || exit 92
        [ "$3" = 'install' ] || exit 93
        [ "$4" = '-y' ] || exit 94
        printf '%s\\n' "$@" > "$PACKAGE_ARGUMENTS" || exit 95
        i=1
        while [ "$i" -le 30 ]; do
            printf 'line-%02d\\n' "$i"
            i=$((i + 1))
        done
        printf 'synthetic-outcome=%s\\n' "$PACKAGE_STATUS" >&2
        exit "$PACKAGE_STATUS"
        """.utf8)
        guard mkdir(root.path, 0o700) == 0 else { throw CocoaError(.fileWriteFileExists) }
        ownsRoot = true
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: false,
                                                attributes: [.posixPermissions: 0o700])
        try script.write(to: bin.appendingPathComponent("sudo"), options: .withoutOverwriting)
        try FileManager.default.setAttributes([.posixPermissions: 0o700],
                                              ofItemAtPath: bin.appendingPathComponent("sudo").path)
        for (name, target) in [("tail", "/usr/bin/tail"), ("bash", "/bin/bash")] {
            try FileManager.default.createSymbolicLink(atPath: bin.appendingPathComponent(name).path,
                                                      withDestinationPath: target)
        }
    }

    deinit { if ownsRoot { try? FileManager.default.removeItem(at: root) } }

    func run(_ command: String) -> (output: String, code: Int32) {
        Shell.run("/usr/bin/env", ["-i", "PATH=\(bin.path)", "LANG=C",
            "PACKAGE_ARGUMENTS=\(argumentsFile.path)", "PACKAGE_STATUS=\(status)",
            "/bin/sh", "-c", command], timeout: 3, outputLimitBytes: 16_384)
    }

    func arguments() throws -> [String] {
        try String(contentsOf: argumentsFile, encoding: .utf8).split(separator: "\n").map(String.init)
    }

    var expectedOutput: String {
        (7...30).map { String(format: "line-%02d\n", $0) }.joined()
            + "synthetic-outcome=\(status)\n"
    }

    func assertUnchanged(file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(try Data(contentsOf: bin.appendingPathComponent("sudo")), script, file: file, line: line)
    }
}

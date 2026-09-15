#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

extension AppUIHostContractTests {
    func fixture() throws -> (root: URL, output: URL) {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-ui-host-contract-" + UUID().uuidString, isDirectory: true)
        let parent = root.appendingPathComponent("app-ui-private", isDirectory: true)
        let output = parent.appendingPathComponent("host-observations", isDirectory: true)
        for directory in [root, parent, output] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                                   attributes: [.posixPermissions: 0o700])
        }
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return (root, output)
    }
    func arguments(_ output: URL) -> [String] { ["--app-ui-host", "--output", output.path] }
    func object(_ output: URL, _ name: String) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: output.appendingPathComponent(name))) as? [String: Any])
    }
    func environment(_ output: URL) -> [String: String] {
        ["BRIDGEVM_APP_UI_HOST_MODE": "1", "BRIDGEVM_APP_UI_HOST_OUTPUT": output.path]
    }
    func assertRequestTransports(output: URL) throws {
        let argumentRequest = try AppUIHostRequest.resolve(arguments: arguments(output), environment: [:])
        XCTAssertEqual(argumentRequest.output, output)
        XCTAssertEqual(argumentRequest.transport, .arguments)
        XCTAssertEqual(argumentRequest.argumentCount, 3)
        let environmentRequest = try AppUIHostRequest.resolve(arguments: [], environment: environment(output))
        XCTAssertEqual(environmentRequest.output, output)
        XCTAssertEqual(environmentRequest.transport, .environment)
        XCTAssertEqual(environmentRequest.argumentCount, 0)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: output.path), [])
    }
    func assertMalformedRequestEnvelopes(output: URL) throws {
        let mode = AppUIHostRequest.modeKey
        let path = AppUIHostRequest.outputKey
        let malformed: [([String], [String: String])] = [
            ([], [mode: "1"]),
            ([], [path: output.path]),
            ([], [mode: "0", path: output.path]),
            ([], [mode: "true", path: output.path]),
            ([], [mode: "", path: output.path]),
            ([], [mode: "1", path: ""]),
            ([], [mode: "1", path: "app-ui-private/host-observations"]),
            ([], [mode: "1", path: output.path + "/../host-observations"]),
            ([], [mode: "1", path: output.path + "\0"]),
            ([], [mode: "1", path: "/" + String(repeating: "x", count: 4096)]),
            (arguments(output), environment(output)),
            (arguments(output), [mode: "1"]),
            (arguments(output), [path: output.path]),
            (["--vtpm-lifecycle"], environment(output))
        ]
        for (arguments, environment) in malformed {
            XCTAssertThrowsError(try AppUIHostRequest.resolve(arguments: arguments, environment: environment))
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: output.path), [])
        }
    }
    func assertEnvironmentRefuses(output: URL, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertThrowsError(try AppUIHostRequest.resolve(arguments: [], environment: environment(output)),
                             file: file, line: line)
    }
}
#endif

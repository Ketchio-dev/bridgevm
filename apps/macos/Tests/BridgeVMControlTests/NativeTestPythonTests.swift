import XCTest
@testable import BridgeVMControl

final class NativeTestPythonTests: XCTestCase {
    func testDirectDeveloperPythonPrecedesPathShim() throws {
        let result = try NativeTestPython.executable(
            environment: ["DEVELOPER_DIR": "/developer", "PATH": "/usr/bin:/fallback"]
        ) { ["/developer/usr/bin/python3", "/usr/bin/python3"].contains($0) }
        XCTAssertEqual(result.path, "/developer/usr/bin/python3")
    }

    func testPathSearchRemainsThePortableFallback() throws {
        let result = try NativeTestPython.executable(
            environment: ["PATH": "/missing:/tools"]
        ) { $0 == "/tools/python3" }
        XCTAssertEqual(result.path, "/tools/python3")
    }

    func testMissingInterpreterFailsClosed() {
        XCTAssertThrowsError(try NativeTestPython.executable(
            environment: ["DEVELOPER_DIR": "relative", "PATH": "/missing"]
        ) { _ in false }) { error in
            XCTAssertEqual((error as? CocoaError)?.code, .executableNotLoadable)
        }
    }
}

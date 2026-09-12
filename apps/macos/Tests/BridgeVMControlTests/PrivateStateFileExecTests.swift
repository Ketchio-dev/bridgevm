import Foundation
import Darwin
import XCTest
@testable import BridgeVMControl

final class PrivateStateFileExecTests: XCTestCase {
    func testExecClosesPrivateDescriptorButKeepsInheritableControl() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let privateFD = try PrivateStateFileDescriptor.create(at: root.appendingPathComponent("private"))
        defer { Darwin.close(privateFD) }
        let controlFD = Darwin.open(root.appendingPathComponent("control").path,
                                    O_WRONLY | O_CREAT | O_EXCL, mode_t(0o600))
        XCTAssertGreaterThanOrEqual(controlFD, 0)
        guard controlFD >= 0 else { return }
        defer { Darwin.close(controlFD) }
        XCTAssertEqual(fcntl(controlFD, F_GETFD) & FD_CLOEXEC, 0)
        let script = "if [ -e /dev/fd/\(privateFD) ]; then exit 41; fi; " +
            "if [ ! -e /dev/fd/\(controlFD) ]; then exit 42; fi; exit 0"
        var arguments = ["/bin/sh", "-c", script].map { $0.withCString { strdup($0) } }
        defer { for argument in arguments { if let argument { free(argument) } } }
        guard arguments.allSatisfy({ $0 != nil }) else {
            XCTFail("Unable to allocate child arguments")
            return
        }
        arguments.append(nil)
        var environment: [UnsafeMutablePointer<CChar>?] = [nil]
        var pid: pid_t = 0
        let launched = posix_spawn(&pid, "/bin/sh", nil, nil, &arguments, &environment)
        XCTAssertEqual(launched, 0)
        guard launched == 0 else { return }
        var status: Int32 = 0
        var waited: pid_t
        repeat { waited = waitpid(pid, &status, 0) } while waited == -1 && errno == EINTR
        XCTAssertEqual(waited, pid)
        XCTAssertEqual(status, 0, "41 means private FD inherited; 42 means control was not inherited")
    }
}

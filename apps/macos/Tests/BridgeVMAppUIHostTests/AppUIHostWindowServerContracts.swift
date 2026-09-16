#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

extension AppUIHostContractTests {
    func assertWindowServerCapturePolicy() throws {
        typealias Policy = AppUIHostWindowServerPolicy
        let own = Policy.Candidate(windowID: 7, processID: 42, onScreen: true)
        let other = Policy.Candidate(windowID: 8, processID: 900, onScreen: true)
        XCTAssertEqual(try Policy.select([other, own], windowNumber: 7, processID: 42), 1)
        let invalid: [[Policy.Candidate]] = [[], [other], [own, own],
            [.init(windowID: 7, processID: nil, onScreen: true)],
            [.init(windowID: 7, processID: 900, onScreen: true)],
            [.init(windowID: 7, processID: 42, onScreen: false)],
            [own, .init(windowID: 7, processID: 900, onScreen: true)],
            Array(repeating: other, count: 256) + [own]]
        for candidates in invalid {
            XCTAssertThrowsError(try Policy.select(candidates, windowNumber: 7, processID: 42))
        }
        for number in [0, -1, Int(UInt32.max) + 1] {
            XCTAssertThrowsError(try Policy.select([own], windowNumber: number, processID: 42))
        }
        XCTAssertThrowsError(try Policy.select([own], windowNumber: 7, processID: 1))
        XCTAssertEqual(try Policy.dimensions(x: -20, y: 30, width: 1320, height: 860, scale: 2),
                       .init(width: 2640, height: 1720))
        XCTAssertEqual(try Policy.dimensions(x: 0, y: 0, width: 1.2, height: 1.1, scale: 1),
                       .init(width: 2, height: 2))
        XCTAssertEqual(try Policy.dimensions(x: 0, y: 0, width: 8192, height: 1024, scale: 1),
                       .init(width: 8192, height: 1024))
        for values in [[Double.nan, 0, 1, 1, 1], [0, Double.infinity, 1, 1, 1],
                       [0, 0, 0, 1, 1], [0, 0, -1, 1, 1], [0, 0, 1, 1, 0],
                       [0, 0, 1, 1, Double.nan], [0, 0, Double.infinity, 1, 1],
                       [0, 0, 1, 1, Double.infinity], [0, 0, 8193, 1, 1],
                       [0, 0, 8192, 1025, 1], [0, 0, Double.greatestFiniteMagnitude, 1, 2]] {
            XCTAssertThrowsError(try Policy.dimensions(x: values[0], y: values[1], width: values[2],
                                                      height: values[3], scale: values[4]))
        }
        try assertWindowServerOutputOwnership()
    }

    private func assertWindowServerOutputOwnership() throws {
        typealias Output = AppUIHostWindowServerPolicy.Output
        let fresh = try fixture()
        let output = try Output(parent: fresh.output)
        XCTAssertEqual(output.directory.lastPathComponent, "window-server")
        XCTAssertEqual((try FileManager.default.attributesOfItem(atPath: output.directory.path)[.posixPermissions] as? NSNumber)?.intValue, 0o700)
        let bytes = Data("owned output guard fixture; not a screenshot".utf8)
        try output.write(bytes, name: "owned-dark-default.png")
        XCTAssertThrowsError(try output.write(Data("replacement".utf8), name: "owned-dark-default.png"))
        XCTAssertEqual(try Data(contentsOf: output.directory.appendingPathComponent("owned-dark-default.png")), bytes)
        XCTAssertThrowsError(try Output(parent: fresh.output))
        XCTAssertThrowsError(try output.write(bytes, name: "../outside.png"))
        XCTAssertThrowsError(try output.write(Data(), name: "owned-dark-default.json"))
        XCTAssertThrowsError(try output.write(Data(repeating: 0, count: 65_537), name: "owned-dark-default.json"))
        let outside = fresh.root.appendingPathComponent("retained.txt")
        try bytes.write(to: outside)
        try FileManager.default.createSymbolicLink(at: output.directory.appendingPathComponent("owned-dark-default.json"),
                                                 withDestinationURL: outside)
        XCTAssertThrowsError(try output.write(Data("replacement".utf8), name: "owned-dark-default.json"))
        XCTAssertEqual(try Data(contentsOf: outside), bytes)
        let replaced = fresh.output.appendingPathComponent("retained-window-server", isDirectory: true)
        try FileManager.default.moveItem(at: output.directory, to: replaced)
        try FileManager.default.createDirectory(at: output.directory, withIntermediateDirectories: false,
                                               attributes: [.posixPermissions: 0o700])
        XCTAssertThrowsError(try output.write(bytes, name: "owned-dark-default.json"))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: output.directory.path), [])
        let linked = try fixture()
        try FileManager.default.createSymbolicLink(at: linked.output.appendingPathComponent("window-server"),
                                                 withDestinationURL: linked.root.appendingPathComponent("absent"))
        XCTAssertThrowsError(try Output(parent: linked.output))
        let nonprivate = try fixture()
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: nonprivate.output.path)
        XCTAssertThrowsError(try Output(parent: nonprivate.output))
    }
}
#endif

import XCTest

/// Every SwiftUI `Menu` the app declares is the shared VM actions menu, which sets an
/// accessibility identifier. The d6 live run of 2026-10-23 at seal 22678cd5 found an
/// `AXMenuButton` of the seeded library refusing every `AXIdentifier` read with
/// kAXErrorFailure (-25200) while no menu set one.
final class LibraryMenuIdentifierSourceTests: XCTestCase {
    func testEveryDeclaredMenuIsTheIdentifiedActionsMenu() throws {
        let sources = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/BridgeVMControl", isDirectory: true)
        let files = try FileManager.default.contentsOfDirectory(at: sources, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "swift" }
        XCTAssertGreaterThan(files.count, 100, "the app sources were found")
        var declared: [String] = []
        for file in files {
            let lines = try String(contentsOf: file, encoding: .utf8).components(separatedBy: "\n")
            for (index, line) in lines.enumerated()
            where line.range(of: #"^\s*Menu\s*[{(]"#, options: .regularExpression) != nil {
                declared.append("\(file.lastPathComponent):\(index + 1)")
            }
        }
        XCTAssertEqual(declared.map { $0.components(separatedBy: ":")[0] }, ["LibraryVMActionsMenu.swift"],
                       "every Menu is the shared actions menu; found \(declared.sorted())")
        let menu = try String(contentsOf: sources.appendingPathComponent("LibraryVMActionsMenu.swift"), encoding: .utf8)
        XCTAssertTrue(menu.contains(".accessibilityIdentifier(Self.identifier)"), "the shared menu sets its identifier")
        XCTAssertTrue(menu.contains("static let identifier = \"bridgevm.library.vm-actions\""))
        for (name, count) in [("LibraryWorkspaceList.swift", 1), ("LibraryVMCard.swift", 1)] {
            let text = try String(contentsOf: sources.appendingPathComponent(name), encoding: .utf8)
            XCTAssertEqual(text.components(separatedBy: "LibraryVMActionsMenu(").count - 1, count,
                           "\(name) shows the shared actions menu")
        }
    }
}

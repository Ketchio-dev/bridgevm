import XCTest

/// Window menus need an identifier for AXIdentifier reads on their AXMenuButtons.
/// The Windows Apps submenu in the menu bar also exposes its own identifier.
final class LibraryMenuIdentifierSourceTests: XCTestCase {
    func testEveryDeclaredMenuHasAnAccessibilityIdentifier() throws {
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
        XCTAssertEqual(declared.map { $0.components(separatedBy: ":")[0] }.sorted(),
                       ["HvfVirtualMachineMenu.swift", "LibraryDashboardActionBar.swift", "LibraryVMActionsMenu.swift"],
                       "check the identifier for every declared Menu; found \(declared.sorted())")
        let bar = try String(contentsOf: sources.appendingPathComponent("HvfVirtualMachineMenu.swift"), encoding: .utf8)
        XCTAssertTrue(bar.contains("Menu(\"Windows Apps\")") && bar.contains(".accessibilityIdentifier(\"runtime.windowsApps\")"),
                      "the menu bar's Windows Apps submenu sets its identifier")
        let dashboard = try String(contentsOf: sources.appendingPathComponent("LibraryDashboardActionBar.swift"), encoding: .utf8)
        XCTAssertTrue(dashboard.contains(".accessibilityIdentifier(\"bridgevm.dashboard.actions\")"),
                      "the dashboard's More Actions menu sets its identifier")
        let menu = try String(contentsOf: sources.appendingPathComponent("LibraryVMActionsMenu.swift"), encoding: .utf8)
        XCTAssertTrue(menu.contains(".accessibilityIdentifier(Self.identifier)"), "the shared menu sets its identifier")
        XCTAssertTrue(menu.contains("static let identifier = \"bridgevm.library.vm-actions\""))
        let workspace = try String(contentsOf: sources.appendingPathComponent("LibraryWorkspaceList.swift"), encoding: .utf8)
        XCTAssertEqual(workspace.components(separatedBy: "LibraryVMActionsMenu(").count - 1, 1,
                       "the live workspace shows the shared actions menu")
    }
}

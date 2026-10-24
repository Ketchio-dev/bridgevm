import SwiftUI
import XCTest
@testable import BridgeVMControl

/// Through the real view values (no window): the actions menu's accessibility identifier
/// is attached to the Menu itself, and the overview card shows that menu, not a bare one.
@MainActor
final class LibraryVMActionsMenuValueTests: XCTestCase {
    func testTheIdentifierIsAttachedToTheMenuItselfAndTheCardShowsIt() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-actions-menu-" + UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        let config = VMConfig(id: "amber", name: "Amber", displayName: "Amber",
            backendKind: "hvf-engine", bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("amber/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "owned", displayWidth: 1280, displayHeight: 800)
        let menu = LibraryVMActionsMenu(library: library, config: config, size: CGSize(width: 30, height: 30))
        XCTAssertEqual(LibraryVMActionsMenu.identifier, "bridgevm.library.vm-actions")
        let identified = attached("\"bridgevm.library.vm-actions\"", in: menu.body)
        XCTAssertEqual(identified.count, 1, "one accessibility attachment carries the identifier")
        XCTAssertEqual(identified.map { menus(in: $0) }, [1], "and it wraps the Menu itself")
        XCTAssertEqual(menus(in: menu.body), 1)
        let card = LibraryVMCard(library: library, config: config).body
        XCTAssertEqual(values(LibraryVMActionsMenu.self, in: card).count, 1, "the card shows the shared menu")
        XCTAssertEqual(menus(in: card), 0, "and no bare Menu")
    }

    /// The content of every modifier whose dump contains `text`.
    private func attached(_ text: String, in value: Any, depth: Int = 0) -> [Any] {
        let mirror = Mirror(reflecting: value)
        if String(describing: Swift.type(of: value)).hasPrefix("ModifiedContent<"),
           let modifier = mirror.children.first(where: { $0.label == "modifier" })?.value,
           let content = mirror.children.first(where: { $0.label == "content" })?.value {
            var dumped = ""; dump(modifier, to: &dumped)
            if dumped.contains(text) { return [content] + attached(text, in: content, depth: depth + 1) }
        }
        guard depth < 64, mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { attached(text, in: $0.value, depth: depth + 1) }
    }
    private func menus(in value: Any, depth: Int = 0) -> Int {
        if String(describing: Swift.type(of: value)).hasPrefix("Menu<") { return 1 }
        let mirror = Mirror(reflecting: value)
        guard depth < 64, mirror.displayStyle != .class else { return 0 }
        return mirror.children.reduce(0) { $0 + menus(in: $1.value, depth: depth + 1) }
    }
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}

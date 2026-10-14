import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// Installed SwiftUI bindings and synthetic control callbacks only. The window
/// is deferred and never shown, focused or used for WindowServer interaction.
@MainActor
enum FirstRunOptionalRecoveryFixture {
    static func run(_ body: (FirstRunImportTransactionFixture, LibraryModel,
        NSHostingView<FirstRunView>) async throws -> Void) async throws {
        let fixture = try FirstRunImportTransactionFixture()
        let library = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        defer { if !library.firstRunImportBusy { fixture.remove() } }
        let repo = fixture.root.appendingPathComponent("tool-repository")
        let wrapper = repo.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        for directory in [wrapper.deletingLastPathComponent(), helper.deletingLastPathComponent()] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try Data("#!/bin/sh\nexit 99\n".utf8).write(to: wrapper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: wrapper.path)
        try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
        let previous = ProcessInfo.processInfo.environment["BRIDGEVM_REPO_ROOT"]
        setenv("BRIDGEVM_REPO_ROOT", repo.path, 1)
        defer {
            if let previous { setenv("BRIDGEVM_REPO_ROOT", previous, 1) }
            else { unsetenv("BRIDGEVM_REPO_ROOT") }
        }
        XCTAssertEqual(HvfMediaImportHelper.bundled.path, helper.path)
        library.selectedID = LibraryModel.firstRunImportSelectionID
        let host = NSHostingView(rootView: FirstRunView(library: library))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 850),
            styleMask: [], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        await settle(host)
        let initial = fields(host)
        XCTAssertEqual(initial.count, 4)
        guard initial.count == 4 else { return }
        edit(initial[0], to: "imported")
        edit(initial[1], to: fixture.disk.path)
        edit(initial[2], to: fixture.vars.path)
        await settle(host)
        try await body(fixture, library, host)
    }

    static func submit(_ host: NSView, library: LibraryModel) async throws {
        let enter = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
            timestamp: 0, windowNumber: 0, context: nil, characters: "\r",
            charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 0x24))
        XCTAssertTrue(host.performKeyEquivalent(with: enter), "Synthetic default action must invoke actual view submission")
        for _ in 0..<3000 {
            if library.firstRunImport.stage != nil && !library.firstRunImportBusy { break }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertNotNil(library.firstRunImport.stage)
        XCTAssertFalse(library.firstRunImportBusy, "An unfinished worker retains its owned fixture for diagnosis")
    }

    static func settle(_ host: NSView) async {
        for _ in 0..<5 { await Task.yield() }
        host.layoutSubtreeIfNeeded()
    }

    static func edit(_ field: NSTextField, to value: String) {
        field.stringValue = value
        field.delegate?.controlTextDidChange?(Notification(name: NSControl.textDidChangeNotification, object: field))
    }

    static func fields(_ view: NSView) -> [NSTextField] {
        let own = (view as? NSTextField).map { $0.isEditable ? [$0] : [] } ?? []
        return own + view.subviews.flatMap(fields)
    }
}

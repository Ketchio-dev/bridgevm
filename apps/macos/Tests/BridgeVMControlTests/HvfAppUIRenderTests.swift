import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfAppUIRenderTests: XCTestCase {
    func testOwnedNativeAppViewsAndAccessibilityActions() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["BRIDGEVM_APP_UI_DIAGNOSTIC"] == "1" else {
            throw XCTSkip("Opt-in WindowServer diagnostic; ordinary deterministic checks do not create NSApplication")
        }
        guard let path = environment["BRIDGEVM_APP_UI_OUTPUT"], path.hasPrefix("/"), path != "/" else {
            throw HvfAppUIError.refused("A fresh absolute owned output directory is required")
        }
        let capture = try HvfAppUICapture(output: URL(fileURLWithPath: path, isDirectory: true))
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        do {
            let fixture = try HvfAppUIFixture(capture: capture)
            defer { fixture.close() }
            try await runScenario(fixture)
            try fixture.assertNoLibraryWrites()
            try capture.save()
        } catch {
            capture.failed(error)
            throw error
        }
    }

    private func runScenario(_ fixture: HvfAppUIFixture) async throws {
        let content = fixture.content
        let capture = fixture.capture
        try await HvfAppUIAccessibility.wait("welcome controls") {
            try HvfAppUIAccessibility.find("bridgevm.first-run.create", in: content) != nil
                && HvfAppUIAccessibility.find("bridgevm.first-run.import", in: content) != nil
        }
        try capture.action("welcome_visible")
        for dark in [false, true] {
            for minimum in [false, true] {
                try await fixture.setPresentation(dark: dark, minimum: minimum)
                try capture.capture(content, name: "welcome-\(dark ? "dark" : "light")-\(minimum ? "minimum" : "default")")
            }
        }
        try await fixture.setPresentation(dark: false, minimum: false)
        try HvfAppUIAccessibility.press("bridgevm.first-run.create", in: content)
        try await HvfAppUIAccessibility.wait("creation sheet") {
            fixture.library.showingCreate && fixture.window.sheets.count == 1
        }
        guard let sheetContent = fixture.window.sheets.first?.contentView else {
            throw HvfAppUIError.refused("Actual creation sheet has no owned content")
        }
        try await HvfAppUIAccessibility.wait("creation form") {
            try HvfAppUIAccessibility.find("bridgevm.create.commit", in: sheetContent) != nil
        }
        for field in ["iso", "guest-payload", "guest-manifest"] {
            let identifier = "bridgevm.create.windows.\(field).selection"
            guard let element = try HvfAppUIAccessibility.find(identifier, in: sheetContent),
                  element.accessibilityValue() as? String == ""
            else { throw HvfAppUIError.refused("Creation capture requires empty source selections") }
        }
        try capture.capture(sheetContent, name: "create-sheet")
        try capture.action("create_opened")
        try HvfAppUIAccessibility.pressButton(label: "취소", in: sheetContent)
        try await HvfAppUIAccessibility.wait("creation sheet cancellation") {
            !fixture.library.showingCreate && fixture.window.sheets.isEmpty
        }
        try capture.action("create_cancelled")
        try HvfAppUIAccessibility.press("bridgevm.first-run.import", in: content)
        try await HvfAppUIAccessibility.wait("import form") {
            try fixture.library.selectedID == LibraryModel.firstRunImportSelectionID
                && HvfAppUIAccessibility.find("bridgevm.first-run.name", in: content) != nil
        }
        try capture.capture(content, name: "import-form")
        try capture.action("import_opened")
        fixture.seedOverviewMetadata()
        try HvfAppUIAccessibility.press("bridgevm.library.overview", in: content)
        try await HvfAppUIAccessibility.wait("overview cards") {
            try fixture.library.proMode && fixture.library.selectedID == nil
                && HvfAppUIAccessibility.hasCard(named: "Amber", in: content)
                && HvfAppUIAccessibility.hasCard(named: "Indigo", in: content)
        }
        try capture.capture(content, name: "overview")
        try capture.action("overview_opened")
        try HvfAppUIAccessibility.setText("Indigo", identifier: "bridgevm.library.search", in: content)
        try await HvfAppUIAccessibility.wait("search results") {
            try HvfAppUIAccessibility.hasCard(named: "Indigo", in: content)
                && !HvfAppUIAccessibility.hasCard(named: "Amber", in: content)
        }
        try capture.capture(content, name: "overview-search")
        try capture.action("search_filtered")
        try HvfAppUIAccessibility.pressButton(label: "검색 지우기", in: content)
        try await HvfAppUIAccessibility.wait("cleared search") {
            try HvfAppUIAccessibility.hasCard(named: "Amber", in: content)
                && HvfAppUIAccessibility.hasCard(named: "Indigo", in: content)
                && HvfAppUIAccessibility.find("bridgevm.library.search", in: content)?.accessibilityValue() as? String == ""
        }
        try capture.action("search_cleared")
    }
}

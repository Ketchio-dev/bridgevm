#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostScenario {
    static func run(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        let capture = host.capture
        try await AppUIHostWelcomeMatrix.run(host, window: window, content: content)
        try await AppUIHostWindow.setPresentation(window, dark: false, minimum: false)
        try await AppUIHostWelcomeControls.wait(host, window: window, content: content)
        try capture.action("welcome_visible")
        try AppUIHostAccessibility.press("bridgevm.first-run.create", in: content)
        try await AppUIHostAccessibility.wait("creation sheet") {
            host.library.showingCreate && window.sheets.count == 1
        }
        guard let sheetContent = window.sheets.first?.contentView else {
            throw AppUIHostError.refused("Actual creation sheet has no owned content")
        }
        try await AppUIHostAccessibility.wait("creation form") {
            try AppUIHostAccessibility.find("bridgevm.create.commit", in: sheetContent) != nil
        }
        for field in ["iso", "guest-payload", "guest-manifest"] {
            let identifier = "bridgevm.create.windows.\(field).selection"
            guard let element = try AppUIHostAccessibility.find(identifier, in: sheetContent),
                  element.accessibilityValue() as? String == ""
            else { throw AppUIHostError.refused("Creation capture requires empty source selections") }
        }
        try capture.capture(sheetContent, name: "create-sheet")
        try capture.action("create_opened")
        try AppUIHostAccessibility.pressButton(label: "취소", in: sheetContent)
        try await AppUIHostAccessibility.wait("creation sheet cancellation") {
            !host.library.showingCreate && window.sheets.isEmpty
        }
        try capture.action("create_cancelled")
        try AppUIHostAccessibility.press("bridgevm.first-run.import", in: content)
        try await AppUIHostAccessibility.wait("import form") {
            try host.library.selectedID == LibraryModel.firstRunImportSelectionID
                && AppUIHostAccessibility.find("bridgevm.first-run.name", in: content) != nil
        }
        try capture.capture(content, name: "import-form")
        try capture.action("import_opened")
        host.seedOverviewMetadata()
        try AppUIHostAccessibility.press("bridgevm.library.overview", in: content)
        try await AppUIHostAccessibility.wait("overview cards") {
            try host.library.proMode && host.library.selectedID == nil
                && AppUIHostAccessibility.hasCard(named: "Amber", in: content)
                && AppUIHostAccessibility.hasCard(named: "Indigo", in: content)
        }
        try capture.capture(content, name: "overview")
        try capture.action("overview_opened")
        try AppUIHostAccessibility.setText("Indigo", identifier: "bridgevm.library.search", in: content)
        try await AppUIHostAccessibility.wait("search results") {
            try AppUIHostAccessibility.hasCard(named: "Indigo", in: content)
                && !AppUIHostAccessibility.hasCard(named: "Amber", in: content)
        }
        try capture.capture(content, name: "overview-search")
        try capture.action("search_filtered")
        try AppUIHostAccessibility.pressButton(label: "검색 지우기", in: content)
        try await AppUIHostAccessibility.wait("cleared search") {
            try AppUIHostAccessibility.hasCard(named: "Amber", in: content)
                && AppUIHostAccessibility.hasCard(named: "Indigo", in: content)
                && AppUIHostAccessibility.find("bridgevm.library.search", in: content)?.accessibilityValue() as? String == ""
        }
        try capture.action("search_cleared")
    }
}
#endif

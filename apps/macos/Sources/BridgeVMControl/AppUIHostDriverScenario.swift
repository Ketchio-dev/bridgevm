#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostDriverScenario {
    static func requested(environment: [String: String] = ProcessInfo.processInfo.environment) throws -> Bool {
        guard let mode = environment["BRIDGEVM_APP_UI_DRIVER_MODE"] else { return false }
        guard mode == "1" else { throw AppUIDriverFailure.invalidRequest }
        return true
    }
    private static func deadline() -> Double { ProcessInfo.processInfo.systemUptime + 5 }

    static func run(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        let capture = host.capture
        window.setAccessibilityIdentifier(AppUIDriverConstants.mainWindowIdentifier)
        try await AppUIHostWelcomeMatrix.run(host, window: window, content: content)
        try await AppUIHostWindow.setPresentation(window, dark: false, minimum: false)
        let welcome = deadline()
        let client = try await AppUIHostDriverClient.connect(host, window: window, content: content, deadline: welcome)
        try await AppUIHostDriverClient.wait("welcome controls", deadline: welcome) {
            let value = try await client.perform(.welcomeControls, phase: .welcome, deadline: welcome)
            return value?.createVisible == true && value?.importVisible == true
        }
        try capture.action("welcome_visible")

        let creation = deadline()
        try await client.perform(.pressCreate, phase: .creation, deadline: creation)
        try await AppUIHostDriverClient.wait("creation sheet", deadline: creation) {
            host.library.showingCreate && window.sheets.count == 1
        }
        guard let sheet = window.sheets.first, let sheetContent = sheet.contentView,
              sheet.sheetParent === window else { throw AppUIHostError.refused("Actual creation sheet has no owned content") }
        sheet.setAccessibilityIdentifier(AppUIDriverConstants.sheetWindowIdentifier)
        var selections: AppUIDriverValues?
        try await AppUIHostDriverClient.wait("creation form", deadline: creation) {
            guard window.sheets.count == 1, window.sheets.first === sheet,
                  sheet.contentView === sheetContent, sheetContent.window === sheet else {
                throw AppUIDriverFailure.identityMismatch
            }
            selections = try await client.perform(.createForm, phase: .creation, deadline: creation)
            return selections?.commitVisible == true
        }
        guard selections?.isoSelection == "", selections?.payloadSelection == "", selections?.manifestSelection == "" else {
            throw AppUIHostError.refused("Creation capture requires empty source selections")
        }
        try capture.capture(sheetContent, name: "create-sheet")
        try capture.action("create_opened")

        let cancellation = deadline()
        try await client.perform(.cancelCreate, phase: .cancellation, deadline: cancellation)
        try await AppUIHostDriverClient.wait("creation sheet cancellation", deadline: cancellation) {
            !host.library.showingCreate && window.sheets.isEmpty
        }
        try capture.action("create_cancelled")

        let importing = deadline()
        try await client.perform(.pressImport, phase: .importing, deadline: importing)
        try await AppUIHostDriverClient.wait("import form", deadline: importing) {
            guard host.library.selectedID == LibraryModel.firstRunImportSelectionID else { return false }
            return try await client.perform(.importForm, phase: .importing, deadline: importing)?.nameVisible == true
        }
        try capture.capture(content, name: "import-form")
        try capture.action("import_opened")
        host.seedOverviewMetadata()

        let overview = deadline()
        try await client.perform(.pressOverview, phase: .overview, deadline: overview)
        try await AppUIHostDriverClient.wait("overview cards", deadline: overview) {
            guard host.library.proMode && host.library.selectedID == nil else { return false }
            let value = try await client.perform(.overviewCards, phase: .overview, deadline: overview)
            return value?.amberVisible == true && value?.indigoVisible == true
        }
        try capture.capture(content, name: "overview")
        try capture.action("overview_opened")

        let filtering = deadline()
        try await client.perform(.setSearchIndigo, phase: .filtering, deadline: filtering)
        try await AppUIHostDriverClient.wait("search results", deadline: filtering) {
            let value = try await client.perform(.searchState, phase: .filtering, deadline: filtering)
            return value?.indigoVisible == true && value?.amberVisible == false && value?.searchValue == "Indigo"
        }
        try capture.capture(content, name: "overview-search")
        try capture.action("search_filtered")

        let clearing = deadline()
        try await client.perform(.clearSearch, phase: .clearing, deadline: clearing)
        try await AppUIHostDriverClient.wait("cleared search", deadline: clearing) {
            let value = try await client.perform(.searchState, phase: .clearing, deadline: clearing)
            return value?.amberVisible == true && value?.indigoVisible == true && value?.searchValue == ""
        }
        try capture.action("search_cleared")
    }
}
#endif

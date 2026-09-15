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
        do {
            let bootstrap = try HvfAppUIBootstrap(output: capture.output)
            defer { bootstrap.close() }
            let fixture = try HvfAppUIFixture(capture: capture)
            defer { fixture.close() }
            try await HvfAppUIScenario.run(fixture)
            try fixture.assertNoLibraryWrites()
            try bootstrap.assertNoDocumentRequests()
            try capture.save()
        } catch {
            capture.failed(error)
            throw error
        }
    }
}

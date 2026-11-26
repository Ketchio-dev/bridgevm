import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class CloneWindowsHVFSheetTests: XCTestCase {
    func testSuggestedNameCanBeUsedWhenTheSourceNameAlreadyFillsTheLimit() async throws {
        let fixture = try VMCloneDestinationFixture()
        let library = LibraryModel(rootURL: fixture.library, migrateLegacy: false, startsModelsAutomatically: false)
        for sourceName in [String(repeating: "A", count: VMLibrary.maximumVMNameCharacters),
                           String(repeating: "한", count: VMLibrary.maximumVMSlugBytes / 3)] {
            var source = fixture.config(windows: true)
            source.name = sourceName
            XCTAssertEqual(VMLibrary.normalizedVMName(source.name), sourceName)
            let host = NSHostingView(rootView: CloneWindowsHVFSheet(library: library, config: source))
            let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 480, height: 320),
                                  styleMask: [], backing: .buffered, defer: true)
            window.contentView = host
            await FirstRunOptionalRecoveryFixture.settle(host)
            let field = try XCTUnwrap(FirstRunOptionalRecoveryFixture.fields(host).first)
            XCTAssertTrue(field.stringValue.hasSuffix(" Copy"))
            XCTAssertTrue(field.stringValue.hasPrefix(String(sourceName.prefix(10))))
            XCTAssertEqual(VMLibrary.normalizedVMName(field.stringValue), field.stringValue)
            window.contentView = nil
        }
    }
}

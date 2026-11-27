import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryRuntimeDetailContentTests: XCTestCase {
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    func testStaleHvfRegistrationShowsUnavailableDetail() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let original = fixture.config()
        fixture.save(original)
        let library = fixture.library()
        let model = try XCTUnwrap(library.selectedModel)
        model.running = true
        var replacement = original
        replacement.backendKind = "fast-vz"
        fixture.save(replacement)
        library.reload()
        let body = LibraryDetailView(library: library).body
        XCTAssertEqual(values(LibraryUnavailableRuntimeDetail.self, in: body).count, 1)
        XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
        XCTAssertEqual(fixture.runtimeConfigs.count, 0)
    }

    func testRetiredEngineRegistrationShowsUnavailableDetail() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        var config = fixture.config()
        config.backendKind = "fast-vz"
        fixture.save(config)
        let library = fixture.library()

        let body = LibraryRuntimeDetailContent(config: config, library: library).body
        XCTAssertEqual(values(LibraryUnavailableRuntimeDetail.self, in: body).count, 1)
        XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
        XCTAssertEqual(fixture.runtimeConfigs.count, 0)
    }
}

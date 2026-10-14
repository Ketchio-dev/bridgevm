import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfClipboardSyncImportedPreferenceTests: XCTestCase {
    private func savedOff(_ fixture: HvfRuntimeSessionStoreFixture) throws -> VMConfig {
        let encoded = try JSONEncoder().encode(fixture.config())
        var fields = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        fields["clipboardSyncEnabled"] = false
        return try JSONDecoder().decode(VMConfig.self, from: JSONSerialization.data(withJSONObject: fields))
    }

    func testImportedOffPreferenceReachesDashboardAndBackendLaunchBuilders() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let config = try savedOff(fixture)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: fixture.root))
        XCTAssertFalse(launch.clipboardSync)
        XCTAssertFalse(launch.wrapperArguments().contains("--agent-clipboard-sync"))
        var pending = config
        pending.installPending = true
        let backend = HvfWindowsBackend(pending, processIsRunning: { _ in false }, libraryRoot: fixture.root)
        XCTAssertFalse(backend.makeHvfEngineConfig().clipboardSync)
    }

    func testSavingImportedConfigurationRetainsTheOffPreference() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let bytes = try JSONEncoder().encode(savedOff(fixture))
        let fields = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
        XCTAssertEqual(fields["clipboardSyncEnabled"] as? Bool, false)
    }

    func testExistingRegistrationWithoutPreferenceKeepsDefaultOn() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(fixture.config(), rootURL: fixture.root))
        XCTAssertTrue(launch.clipboardSync)
        XCTAssertTrue(launch.wrapperArguments().contains("--agent-clipboard-sync"))
    }
}

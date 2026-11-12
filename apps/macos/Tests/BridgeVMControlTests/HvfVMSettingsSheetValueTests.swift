import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfVMSettingsSheetValueTests: XCTestCase {
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func assertMoved<T>(_ type: T.Type, sheet: Any, advanced: Any,
                                file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(values(type, in: sheet).count, 1, "one \(type) in settings", file: file, line: line)
        XCTAssertEqual(values(type, in: advanced).count, 0, "no \(type) in advanced", file: file, line: line)
    }

    func testMemoryUsesPlainGBAndDriverExplainsUnavailableState() throws {
        XCTAssertEqual(HvfDiskSizeSetting.sizeText(64 << 30), "64GB")
        XCTAssertEqual(HvfDiskSizeSetting.sizeText(3 << 29), "1.5GB")
        XCTAssertEqual(HvfResourceSetting.memoryText(6144), "6GB")
        XCTAssertEqual(HvfResourceSetting.memoryText(1536), "1.50GB")
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertEqual(HvfDisplayDriverInstall.unavailableReason(session), "Windows를 시작하고 연결될 때까지 기다리세요.")
        session.connectionState = .connected(host: "fixture")
        XCTAssertEqual(HvfDisplayDriverInstall.unavailableReason(session), "Windows 도우미가 준비될 때까지 기다리세요.")
        session.serviceStarted = true
        XCTAssertNil(HvfDisplayDriverInstall.unavailableReason(session))
        session.connectionState = .stopped
    }

    func testEachSettingOccursOnceInSheetAndNotInAdvanced() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        var saved = fixture.config()
        saved.virtioDisplayDriverInstalled = true
        saved.networkEnabled = true
        fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let sheet = HvfVMSettingsSheet(session: session).body
        let advanced = HvfEngineView(session: session).body
        assertMoved(HvfStartCloseSettings.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfResourceSetting.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfDiskSizeSetting.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfRetinaResolutionToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfGuestResolutionPicker.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfDisplayDriverInstallButton.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfHardwareCursorToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfSharedFolderSettings.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfClipboardSyncToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfGuestPrintToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfGuestLinksToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfNetworkToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfPortForwardSetting.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfCommandKeyPicker.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfMicrophoneToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfDurableDiskWritesToggle.self, sheet: sheet, advanced: advanced)
        assertMoved(HvfWindowsSnapshotCard.self, sheet: sheet, advanced: advanced)
    }

    func testAutoStartUsesExistingRegistrationSavePath() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertNil(session.saveLibraryPreference("자동 시작 설정", publish: { VMRegistrationWriter.commit($0, to: $1) }) {
            $0.startWithBridgeVM = true
        })
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: fixture.registration(saved)))
        XCTAssertEqual(file.startWithBridgeVM, true)
        XCTAssertEqual(session.config.libraryContext?.config.startWithBridgeVM, true)
        XCTAssertEqual(fixture.processLookups, 1, "Saving uses the existing disk-in-use safety check")
        fixture.processLookups = 0
    }
    func testNetworkToggleInSheetUsesExistingSavePath() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.networkEnabled = true; fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let sheet = HvfVMSettingsSheet(session: session).body
        let control = try XCTUnwrap(values(HvfNetworkToggle.self, in: sheet).first)
        // The exact action used by Toggle, not a reflection of SwiftUI's opaque Binding storage.
        control.update(false)
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: fixture.registration(saved)))
        XCTAssertEqual(file.networkEnabled, false)
        XCTAssertFalse(session.config.virtioNet)
        XCTAssertEqual(library.latestConfiguration(for: saved).networkEnabled, false)
        XCTAssertEqual(probe.lookups, [session.config.targetDiskPath])
    }

    func testExternalChangeSurvivesAnotherControlSaveFromAlreadyOpenedSheet() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.networkEnabled = true; fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let sheet = HvfVMSettingsSheet(session: session)
        XCTAssertNil(session.saveLibraryPreference("외부 디스플레이 설정", publish: {
            VMRegistrationWriter.commit($0, to: $1)
        }) { $0.virtioDisplayDriverInstalled = true })
        let network = try XCTUnwrap(values(HvfNetworkToggle.self, in: sheet.body).first)
        network.update(false)
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: fixture.registration(saved)))
        XCTAssertEqual(file.virtioDisplayDriverInstalled, true)
        XCTAssertEqual(file.networkEnabled, false)
        XCTAssertEqual(session.config.libraryContext?.config.virtioDisplayDriverInstalled, true)
    }

    func testRunningSheetShowsNoticeAndKeepsExistingControlDisabling() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.virtioDisplayDriverInstalled = true; fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let sheet = HvfVMSettingsSheet(session: session)
        XCTAssertNil(sheet.runningNoticeText)
        session.connectionState = .booting
        defer { session.connectionState = .stopped }
        let running = sheet.body
        XCTAssertEqual(sheet.runningNoticeText, "일부 설정은 Windows를 종료한 뒤 바꿀 수 있습니다.")
        XCTAssertTrue(try XCTUnwrap(values(HvfResourceSetting.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfNetworkToggle.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfClipboardSyncToggle.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfSharedFolderSettings.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfGuestResolutionPicker.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfRetinaResolutionToggle.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfCommandKeyPicker.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfMicrophoneToggle.self, in: running).first).isDisabled)
        XCTAssertTrue(try XCTUnwrap(values(HvfDurableDiskWritesToggle.self, in: running).first).isDisabled)
        XCTAssertFalse(session.libraryPreferencesEditable, "print, links and cursor keep their own stopped-only guard")
        XCTAssertTrue(session.hasActiveRuntimeWork, "snapshot keeps its running-work guard")
        // Driver installation intentionally remains a running-guest operation.
    }
}

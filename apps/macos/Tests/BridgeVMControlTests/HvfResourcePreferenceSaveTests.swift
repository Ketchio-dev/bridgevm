import SwiftUI
import XCTest
@testable import BridgeVMControl

/// A library VM's RAM and CPU on the runtime page are saved in vm.json (`memMiB`, `cpuCount`) like the
/// other runtime-view preferences, so the dashboard's start uses them; before, the steppers were
/// view-local and a dashboard start silently used vm.json's old values.
@MainActor
final class HvfResourcePreferenceSaveTests: XCTestCase {
    private func dashboardLaunch(_ library: LibraryModel, _ saved: VMConfig) throws -> HvfEngineConfig {
        try XCTUnwrap(HvfEngineConfig.libraryVM(library.latestConfiguration(for: saved), rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
    }

    private func lines(_ data: Data) -> [Substring] {
        String(decoding: data, as: UTF8.self).split(separator: "\n")
            .filter { !$0.contains("\"memMiB\" :") && !$0.contains("\"cpuCount\" :") }
    }

    /// A save of only memMiB/cpuCount keeps the stopped runtime page and its session (the clipboard,
    /// network and other preferences already do): the session is rebound in place with the saved
    /// values, and nothing tells the user the registration changed under the page.
    func testSavingOnlyResourcesRebindsTheSessionInPlace() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertNil(session.saveLibraryPreference("메모리·CPU 설정", publish: { VMRegistrationWriter.commit($0, to: $1) }) {
            $0.memMiB = 8192; $0.cpuCount = 6
        })
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session, "the page keeps its session")
        XCTAssertNil(library.operationError)
        XCTAssertEqual(session.config.ramMiB, 8192, "the header shows the saved value")
        XCTAssertEqual(session.config.smpCpus, 6)
        XCTAssertEqual(session.config.libraryContext?.config.memMiB, 8192)
    }

    func testAStepperChangeIsSavedAndTheDashboardStartUsesIt() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertEqual(try dashboardLaunch(library, saved).ramMiB, 6144)

        XCTAssertTrue(session.applyResources(memMiB: 8192, cpus: 4) == (8192, 4, nil))
        let memory = try Data(contentsOf: url)
        XCTAssertTrue(String(decoding: memory, as: UTF8.self).contains("\"memMiB\" : 8192"))
        XCTAssertFalse(String(decoding: memory, as: UTF8.self).contains("cpuCount"), "an unchanged CPU count is not written")
        XCTAssertTrue(session.applyResources(memMiB: 8192, cpus: 6) == (8192, 6, nil))
        XCTAssertEqual(lines(try Data(contentsOf: url)), lines(original), "every other vm.json line is unchanged")

        let launch = try dashboardLaunch(library, saved)
        XCTAssertEqual(launch.ramMiB, 8192, "the dashboard's start uses the page's value")
        XCTAssertEqual(launch.smpCpus, 6)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertEqual(session.config.ramMiB, 8192)
        XCTAssertEqual(session.config.smpCpus, 6)
        XCTAssertNil(library.operationError)
    }

    func testRefusedWhileRunningOrBusyAndTheSteppersShowTheSavedValues() throws {
        for state in [HvfConnectionState.booting, .connected(host: "fixture"), .stopping, .timedOut] {
            let fixture = try HvfRuntimeSessionStoreFixture()
            defer { fixture.clean() }
            var saved = fixture.config(); saved.memMiB = 4096; fixture.save(saved)
            let url = fixture.registration(saved), original = try Data(contentsOf: url)
            let library = fixture.library()
            let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
            let setting = HvfResourceSetting(session: session, ramMiB: .constant(4096), cpus: .constant(4))
            XCTAssertFalse(setting.isDisabled)
            session.connectionState = state
            XCTAssertTrue(setting.isDisabled, "\(state)")
            XCTAssertTrue(session.applyResources(memMiB: 8192, cpus: 6)
                == (4096, 4, "VM을 완전히 중지한 뒤 메모리·CPU 설정을 변경하세요."), "\(state)")
            XCTAssertEqual(try Data(contentsOf: url), original)
            XCTAssertEqual(try dashboardLaunch(library, saved).ramMiB, 4096)
            session.connectionState = .stopped
        }
    }

    /// The limits the other engines' resource change keeps (VMResourceLimits, as setResources checks).
    func testAValueOutsideTheLimitsIsRefused() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let limits = HvfEngineSession.resourceLimitsMessage
        XCTAssertTrue(session.applyResources(memMiB: 64 * 1024, cpus: 4) == (6144, 4, limits))
        XCTAssertTrue(session.applyResources(memMiB: 6144, cpus: 12) == (6144, 4, limits))
        XCTAssertTrue(session.applyResources(memMiB: 512, cpus: 4) == (6144, 4, limits))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertTrue(probe.lookups.isEmpty, "refused before the save checks")
    }

    func testExperimentalEntryKeepsViewLocalValues() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let library = fixture.library()
        let session = library.experimentalHvfRuntimeSession()
        session.connectionState = .booting
        XCTAssertFalse(HvfResourceSetting(session: session, ramMiB: .constant(6144), cpus: .constant(4)).isDisabled)
        XCTAssertTrue(session.applyResources(memMiB: 48 * 1024, cpus: 16) == (48 * 1024, 16, nil))
        XCTAssertEqual(session.config.ramMiB, 6144, "the experimental default is unchanged by a view-local edit")
        XCTAssertTrue(library.vms.isEmpty)
        session.connectionState = .stopped
    }
}

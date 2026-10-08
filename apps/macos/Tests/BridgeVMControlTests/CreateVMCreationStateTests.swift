import XCTest
@testable import BridgeVMControl

final class CreateVMCreationStateTests: XCTestCase {
    func testUncertainPublicationRequiresExplicitReadbackInsteadOfAnotherCreation() {
        var state = CreateVMCreationState()
        var adoptions = 0
        let result = state.complete(.publishedButUnsynced(config, "retained, sync unconfirmed")) { _ in
            adoptions += 1; return true
        }
        XCTAssertFalse(result.dismiss)
        XCTAssertEqual(result.code, "vm-publication-unconfirmed")
        XCTAssertEqual(result.message, "retained, sync unconfirmed")
        XCTAssertEqual(adoptions, 0)
        XCTAssertFalse(state.permitsCreation)
        XCTAssertEqual(state.published, config)
        XCTAssertFalse(state.recover { _ in adoptions += 1; return false })
        let failure = state.recoveryFailureMessage
        XCTAssertTrue(failure.contains("retained, sync unconfirmed"))
        XCTAssertTrue(failure.contains("불러오지 못했습니다"))
        XCTAssertEqual(state.recoveryFailureMessage, failure)
        XCTAssertFalse(state.permitsCreation)
        XCTAssertEqual(state.published, config)
        XCTAssertTrue(state.recover { saved in
            adoptions += 1; XCTAssertEqual(saved, config); return true
        })
        XCTAssertEqual(adoptions, 2)
        XCTAssertNil(state.published)
        XCTAssertTrue(state.publicationWarning.isEmpty)
    }

    func testConfirmedCreationRetainsConfigWhenReadbackFails() {
        var state = CreateVMCreationState()
        let result = state.complete(.created(config)) { _ in false }
        XCTAssertFalse(result.dismiss)
        XCTAssertEqual(result.code, "library-publication-failed")
        XCTAssertFalse(state.permitsCreation)
        XCTAssertEqual(state.published, config)
    }

    func testUnpublishedFailureAllowsRetryAndSuccessfulAdoptionDismisses() {
        var state = CreateVMCreationState()
        XCTAssertEqual(state.complete(nil) { _ in XCTFail(); return true }.code, "vm-materialization-failed")
        XCTAssertTrue(state.permitsCreation)
        XCTAssertTrue(state.complete(.created(config)) { $0 == config }.dismiss)
        XCTAssertNil(state.published)
    }

    func testCLIUncertainOutcomeRefusesBeforeReadbackAndRetainsFiles() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("creation-outcome-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let sentinel = root.appendingPathComponent("preserved")
        try Data([1, 2, 3]).write(to: sentinel)
        let request = NativeCLICreateWindowsOptions(name: "VM", isoPath: "/synthetic.iso", diskGiB: 64,
            memoryMiB: 6144, cpuCount: 4, resolution: .init(width: 1440, height: 900), networkEnabled: true)
        XCTAssertThrowsError(try NativeCLICreateWindows.create(request, libraryRoot: root,
            creator: { _, _ in .publishedButUnsynced(self.config, "not confirmed") },
            reader: { _, _ in XCTFail("uncertain publication is not CLI success"); return self.config })) { error in
            XCTAssertTrue(error.localizedDescription.contains("preserved"))
            XCTAssertTrue(error.localizedDescription.contains("vm"))
        }
        XCTAssertEqual(try Data(contentsOf: sentinel), Data([1, 2, 3]))
    }

    private var config: VMConfig {
        VMConfig(id: "vm", name: "VM", displayName: "VM", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/fixture/bundle", runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "vm",
            displayWidth: 1440, displayHeight: 900, installPending: true)
    }
}

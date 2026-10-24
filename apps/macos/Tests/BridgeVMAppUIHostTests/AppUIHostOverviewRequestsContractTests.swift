#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

/// The overview phase's driver requests against the host's real fixture library,
/// with a scripted client in place of the mailbox. No overview request is sent
/// before the library model holds both seeded rows. Production seeding assigns
/// the rows synchronously, so a deferred seeding stands in for a late model.
@MainActor
final class AppUIHostOverviewRequestsContractTests: XCTestCase {
    func testPressOverviewWaitsUntilTheModelHoldsBothSeededRows() async throws {
        for mode in ["synchronous", "deferred"] {
            let (library, seed) = try fixture(deferred: mode == "deferred")
            var requests: [(operation: AppUIDriverOperation, seeded: Bool)] = []
            try await AppUIHostOverviewRequests.run(library, seed: seed, deadline: ProcessInfo.processInfo.systemUptime + 5) {
                requests.append(($0, AppUIHostOverviewRequests.seeded(library)))
                return $0 == .overviewCards ? AppUIDriverValues(amberVisible: true, indigoVisible: true) : nil
            }
            XCTAssertEqual(requests.map { $0.operation }, [.pressOverview, .overviewCards], mode)
            XCTAssertEqual(requests.map { $0.seeded }, [true, true], "\(mode): no overview request precedes the seeding")
        }

        let (unseeded, _) = try fixture(deferred: false)
        let partial = try fixture(deferred: false)
        partial.seed()
        partial.library.vms.removeLast()
        for (library, name) in [(unseeded, "no seeded row"), (partial.library, "only the Amber row")] {
            var requests: [AppUIDriverOperation] = []
            do {
                try await AppUIHostOverviewRequests.run(library, seed: {}, deadline: ProcessInfo.processInfo.systemUptime + 0.3) {
                    requests.append($0)
                    return nil
                }
                XCTFail("\(name): the overview phase must refuse")
            } catch AppUIHostError.refused(let reason) {
                XCTAssertEqual(reason, "Timed out waiting for actual UI: overview seeding", name)
            }
            XCTAssertEqual(requests, [], "\(name): pressOverview is never sent within the overview deadline")
        }
    }

    func testSeededMeansBothFixtureRowsEachExactlyOnce() throws {
        let (library, seed) = try fixture(deferred: false)
        XCTAssertFalse(AppUIHostOverviewRequests.seeded(library), "the import form has no rows")
        seed()
        XCTAssertTrue(AppUIHostOverviewRequests.seeded(library), "the production seeding is visible synchronously")
        XCTAssertEqual(library.vms.map(\.slug), ["ui-amber", "ui-indigo"])
        let rows = library.vms
        for (changed, name) in [([rows[0]], "Amber only"), ([rows[1]], "Indigo only"), (rows + [rows[0]], "Amber twice")] {
            library.vms = changed
            XCTAssertFalse(AppUIHostOverviewRequests.seeded(library), name)
        }
    }

    /// A fresh owned library at the import form, the state the real import action leaves.
    private func fixture(deferred: Bool) throws -> (library: LibraryModel, seed: () -> Void) {
        let base = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-ui-host-overview-requests-" + UUID().uuidString, isDirectory: true)
        let output = base.appendingPathComponent("app-ui-private/host-observations", isDirectory: true)
        let root = output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        addTeardownBlock { try? FileManager.default.removeItem(at: base) }
        let capture = try AppUIHostCapture(output: output)
        let library = AppUIHost.makeLibrary(root: root, capture: capture)
        library.selectedID = LibraryModel.firstRunImportSelectionID
        let seed = { AppUIHost.seedOverview(library, root: root, work: capture.work) }
        return (library, deferred ? { _ = Task { @MainActor in seed() } } : seed)
    }
}
#endif

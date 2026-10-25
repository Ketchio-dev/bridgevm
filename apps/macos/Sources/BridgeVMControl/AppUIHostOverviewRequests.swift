#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

/// The overview phase's driver requests. The press is requested only once the
/// library model holds both seeded fixture rows, within the overview deadline,
/// so it never precedes the seeding it depends on. Today's seeding assigns the
/// rows synchronously, so this wait returns on its first check.
@MainActor
enum AppUIHostOverviewRequests {
    /// The fixture rows AppUIHost.seedOverview registers, each exactly once.
    static let seededIDs = ["ui-amber", "ui-indigo"]

    static func seeded(_ library: LibraryModel) -> Bool {
        seededIDs.allSatisfy { id in library.vms.filter { $0.slug == id }.count == 1 }
    }

    static func run(_ library: LibraryModel, seed: () -> Void, deadline: Double,
                    perform: (AppUIDriverOperation) async throws -> AppUIDriverValues?) async throws {
        seed()
        try await AppUIHostDriverClient.wait("overview seeding", deadline: deadline) { seeded(library) }
        _ = try await perform(.pressOverview)
        try await AppUIHostDriverClient.wait("overview rows", deadline: deadline) {
            guard AppUIHostOverviewState.reached(library) else { return false }
            let value = try await perform(.overviewCards)
            return value?.amberVisible == true && value?.indigoVisible == true
        }
    }
}
#endif

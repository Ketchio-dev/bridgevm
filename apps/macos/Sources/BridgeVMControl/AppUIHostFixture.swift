#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import Darwin

@MainActor
extension AppUIHost {
    static func makeLibrary(root: URL, capture: AppUIHostCapture) -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false,
            installSessionFactory: { _ in capture.tripwire("install_creations") },
            runtimeSessionFactory: { _ in capture.tripwire("runtime_creations") },
            actionScheduler: { _ in capture.tripwire("file_jobs") }, startsModelsAutomatically: false,
            modelFactory: { _ in capture.tripwire("model_creations") })
    }
    static func checkCancellation(output: URL) throws {
        let marker = output.deletingLastPathComponent().appendingPathComponent("cancel.requested").path
        var status = stat()
        if lstat(marker, &status) == 0 { throw AppUIHostError.refused("Host canceled by owning launcher") }
        guard errno == ENOENT else { throw AppUIHostError.refused("Cancellation marker could not be inspected") }
    }
}
#endif

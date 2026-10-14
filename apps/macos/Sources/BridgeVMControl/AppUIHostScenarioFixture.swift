#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
struct AppUIHostScenarioFixture {
    let library: LibraryModel
    let installCancel: AppUIInstallCancelFixture?
    let installCancelCapture: AppUIInstallCancelCapture?

    static func make(root: URL, capture: AppUIHostCapture) throws -> Self {
        if try AppUIInstallCancelScenario.requested() {
            let fixture = try AppUIInstallCancelFixture(root: root)
            let library = fixture.library
            let observations = AppUIInstallCancelCapture(output: capture.output, fixture: fixture)
            try observations.save()
            return Self(library: library, installCancel: fixture, installCancelCapture: observations)
        }
        return Self(library: AppUIHost.makeLibrary(root: root, capture: capture),
                    installCancel: nil, installCancelCapture: nil)
    }
}
#endif

#if BRIDGEVM_APP_UI_DRIVER
#error("BRIDGEVM_APP_UI_DRIVER is reserved for the standalone diagnostic launcher")
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST
import Darwin

enum AppUIHostDriverBuildBoundary {
    @MainActor static func prepare() throws {
        if try AppUIHostDriverScenario.requested() { umask(0o077) }
    }
}
#endif

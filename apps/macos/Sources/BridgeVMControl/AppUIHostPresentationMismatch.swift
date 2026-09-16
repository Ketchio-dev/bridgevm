#if DEBUG && BRIDGEVM_APP_UI_HOST
struct AppUIHostPresentationMismatch: Error, CustomStringConvertible {
    var description: String {
        "Actual content did not adopt requested dimensions and appearance"
    }
}
#endif

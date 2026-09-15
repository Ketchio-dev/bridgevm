#if DEBUG && BRIDGEVM_APP_UI_HOST
extension AppUIHostLifecycle {
    enum Event: String, CaseIterable {
        case appInitialization = "app_initialization"
        case appBodyEvaluated = "app_body_evaluated"
        case rootContentConstructed = "root_content_constructed"
        case contentBodyEvaluated = "content_body_evaluated"
        case rootContentAppeared = "root_content_appeared"
        case libraryFactory = "library_factory"
        case delegateDidFinishLaunching = "delegate_did_finish_launching"
        case representableMake = "representable_make"
        case representableUpdate = "representable_update"
        case attachmentNil = "attachment_nil"
        case attachmentNonnull = "attachment_nonnull"
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import SwiftUI

@MainActor
extension AppUIHostLifecycle {
    static func libraryState() -> StateObject<LibraryModel> {
        AppUIHost.prepared?.lifecycle.record(.appInitialization)
        return StateObject(wrappedValue: libraryModel())
    }
    private static func libraryModel() -> LibraryModel {
        guard let host = AppUIHost.prepared else { fatalError("Diagnostic host was not prepared") }
        host.lifecycle.record(.libraryFactory)
        return host.libraryForApplication()
    }
    static func makeAttachmentView() -> NSView {
        AppUIHost.prepared?.lifecycle.record(.representableMake)
        return AppUIHostWindow.AttachmentView(frame: .zero)
    }
}
#endif

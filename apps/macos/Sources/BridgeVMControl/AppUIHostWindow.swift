#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import SwiftUI

@MainActor
struct AppUIHostWindow: NSViewRepresentable {
    final class AttachmentView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            AppUIHost.prepared?.attachmentChanged(window: window)
        }
    }
    func makeNSView(context: Context) -> NSView { AppUIHostLifecycle.makeAttachmentView() }
    func updateNSView(_ nsView: NSView, context: Context) { AppUIHost.prepared?.lifecycle.record(.representableUpdate) }

}
#endif

import AppKit
import UniformTypeIdentifiers
@testable import BridgeVMControl

@MainActor
final class SelectionPanelStub: FileSelectionPanel {
    var allowsMultipleSelection = true
    var canChooseDirectories = false
    var canChooseFiles = true
    var allowedContentTypes: [UTType] = []
    var url: URL?
    var beginCalls = 0
    private var handler: ((NSApplication.ModalResponse) -> Void)?

    func begin(completionHandler handler: @escaping (NSApplication.ModalResponse) -> Void) {
        beginCalls += 1
        self.handler = handler
    }

    func complete(_ response: NSApplication.ModalResponse) {
        let callback = handler
        handler = nil
        callback?(response)
    }
}

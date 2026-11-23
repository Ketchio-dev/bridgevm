import SwiftUI

/// The actions menu beside a VM's workspace row and its overview card, with the only
/// accessibility identifier either menu has: an `AXMenuButton` without one refuses
/// every `AXIdentifier` read with kAXErrorFailure.
struct LibraryVMActionsMenu: View {
    static let identifier = "bridgevm.library.vm-actions"
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    let size: CGSize

    var body: some View {
        Menu {
            LibraryVMContextMenu(library: library, config: config)
        } label: {
            Image(systemName: "ellipsis").frame(width: size.width, height: size.height)
        }
        .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
        .accessibilityLabel("\(config.name) 작업")
        .accessibilityIdentifier(Self.identifier)
    }
}

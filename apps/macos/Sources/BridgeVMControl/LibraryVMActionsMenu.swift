import SwiftUI

/// The actions menu beside a VM's workspace row and its overview card, with the only
/// accessibility identifier either menu has. The d6 live run of 2026-10-23 at seal
/// 22678cd5 found an `AXMenuButton` of the seeded library refusing every
/// `AXIdentifier` read with kAXErrorFailure while neither menu set one.
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

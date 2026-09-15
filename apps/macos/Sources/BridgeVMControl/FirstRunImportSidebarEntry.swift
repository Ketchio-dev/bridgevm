import SwiftUI

struct FirstRunImportSidebarEntry: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
            || library.selectedID == LibraryModel.firstRunImportSelectionID {
            Section("가져오기") {
                Label("VM 가져오기", systemImage: "square.and.arrow.down")
                    .tag(LibraryModel.firstRunImportSelectionID)
                    .accessibilityIdentifier("bridgevm.library.import.status")
                    .help("진행 상태를 확인하거나 가져오기를 계속합니다.")
            }
        }
    }
}

import SwiftUI

/// A completed deletion whose vTPM key could not be archived. The VM is gone, so
/// this is never presented as a deletion failure.
struct LibraryDeletionWarningAlert: ViewModifier {
    @ObservedObject var library: LibraryModel

    func body(content: Content) -> some View {
        content.alert(
            "VM을 삭제했지만 vTPM 키를 보관하지 못했습니다",
            isPresented: Binding(
                get: { library.deletionWarning != nil },
                set: { if !$0 { library.deletionWarning = nil } }
            )
        ) {
            Button("확인") { library.deletionWarning = nil }
        } message: {
            Text(library.deletionWarning ?? "")
        }
    }
}

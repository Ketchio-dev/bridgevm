import SwiftUI

struct LibraryOperationAlerts: ViewModifier {
    @ObservedObject var library: LibraryModel

    func body(content: Content) -> some View {
        content
        .alert(
            "VM 삭제 실패",
            isPresented: Binding(
                get: { library.deletionError != nil },
                set: { if !$0 { library.deletionError = nil } }
            )
        ) {
            Button("확인") { library.deletionError = nil }
        } message: {
            Text(library.deletionError ?? "알 수 없는 오류")
        }
        .alert(
            "VM 복제 실패",
            isPresented: Binding(
                get: { library.cloneError != nil },
                set: { if !$0 { library.cloneError = nil } }
            )
        ) {
            Button("확인") { library.cloneError = nil }
        } message: {
            Text(library.cloneError ?? "알 수 없는 오류")
        }
        .alert(
            "VM 이동 실패",
            isPresented: Binding(
                get: { library.moveError != nil },
                set: { if !$0 { library.moveError = nil } }
            )
        ) {
            Button("확인") { library.moveError = nil }
        } message: {
            Text(library.moveError ?? "알 수 없는 오류")
        }
        .alert(
            "VM 작업을 진행할 수 없습니다",
            isPresented: Binding(
                get: { library.operationError != nil },
                set: { if !$0 { library.operationError = nil } }
            )
        ) {
            Button("확인") { library.operationError = nil }
        } message: {
            Text(library.operationError ?? "알 수 없는 오류")
        }
    }
}

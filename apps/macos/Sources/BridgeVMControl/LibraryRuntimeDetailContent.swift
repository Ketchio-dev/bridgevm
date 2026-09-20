import SwiftUI

struct LibraryRuntimeDetailContent: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel

    @ViewBuilder var body: some View {
        if config.engineKind == .hvfEngine {
            if let session = library.hvfRuntimeDetailSession(for: config) {
                HvfEngineView(session: session).id(ObjectIdentifier(session))
            } else {
                LibraryUnavailableRuntimeDetail(config: config, library: library)
            }
        } else {
            VMDetailPanel(model: model, library: library).id(config.slug)
        }
    }
}

struct LibraryUnavailableRuntimeDetail: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel

    var body: some View {
        ContentUnavailableView {
            Label("HVF 제어를 열 수 없음", systemImage: "exclamationmark.shield")
        } description: {
            Text("저장된 VM 정보와 현재 제어 상태가 일치하지 않습니다. 진행 중인 작업이 끝난 뒤 라이브러리를 다시 읽으세요.")
        } actions: {
            Button("라이브러리 다시 읽기", action: library.reload)
        }
        .navigationTitle(config.name)
        .accessibilityIdentifier("bridgevm.runtime.unavailable")
    }
}

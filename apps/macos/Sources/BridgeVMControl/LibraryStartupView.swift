import SwiftUI

struct LibraryStartupView: View {
    @ObservedObject var startup: LibraryStartupCoordinator
    var body: some View {
        Group {
            if let model = startup.model {
                ContentView(library: model)
            } else {
                VStack(spacing: 16) {
                    if let failure = startup.failure {
                        Image(systemName: "exclamationmark.triangle").font(.largeTitle)
                        Text("라이브러리를 열지 못했습니다").font(.title2)
                        Text(failure).textSelection(.enabled)
                        Button("다시 시도") { Task { await startup.load() } }
                    } else {
                        ProgressView()
                        Text("라이브러리를 확인하고 있습니다").font(.title2)
                        Text("저장된 설치 복구를 확인한 뒤 VM 목록을 엽니다.")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(40)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task { await startup.load() }
    }
}

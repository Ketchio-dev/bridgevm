import SwiftUI

struct RetainedGenericControlView: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("등록 정보가 사라지거나 변경되었을 때 보관한 기존 제어입니다.")
                    .foregroundColor(.secondary)
                Text(LibraryRetainedControlDescriptor.generic(config: config, model: model).statusLabel)
                if model.hasActiveRuntimeWork {
                    Button("정지") { model.stop() }
                        .disabled(!model.retainedStopAvailable)
                        .accessibilityIdentifier("bridgevm.retained.generic.stop")
                    if model.ownedRuntime == nil {
                        Text("이 앱에서 보관한 프로세스 소유권이 없어 안전한 정지를 요청할 수 없습니다. 기존 VM 창에서 종료하세요.")
                            .font(.caption).foregroundColor(.secondary)
                    }
                } else {
                    Button("목록에서 닫기", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.generic.dismiss")
                }
            }.padding(20)
        }
    }
}

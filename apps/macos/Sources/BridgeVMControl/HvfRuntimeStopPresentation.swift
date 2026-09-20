import Foundation

struct HvfRuntimeStopPresentation: Equatable {
    let isEnabled: Bool
    let guidance: String

    static func make(runtimeActive: Bool, lifecycleBusy: Bool, ownsRuntime: Bool) -> Self {
        if lifecycleBusy {
            return Self(isEnabled: false, guidance: "현재 작업이 끝난 뒤 VM을 정지할 수 있습니다.")
        }
        if runtimeActive, !ownsRuntime {
            return Self(isEnabled: false, guidance: "이 앱이 시작하고 소유한 VM만 안전하게 정지할 수 있습니다.")
        }
        if !runtimeActive {
            return Self(isEnabled: false, guidance: "VM이 실행 중이 아닙니다.")
        }
        return Self(isEnabled: true, guidance: "이 앱이 소유한 VM을 정지합니다.")
    }
}

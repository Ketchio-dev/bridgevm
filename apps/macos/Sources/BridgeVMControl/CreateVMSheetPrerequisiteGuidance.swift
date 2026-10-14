import SwiftUI

extension CreateVMSheet {
    var displayedCreationError: String {
        guard needsTemplateGuidance else { return error }
        return "이 방식은 기존 템플릿 VM이 필요합니다. 라이브러리에 템플릿이 없어 생성할 수 없습니다. Windows의 ‘ISO에서 설치’와 ‘설치된 디스크 가져오기’는 템플릿 없이 준비할 수 있습니다."
    }

    var displayedCreationFailureCode: String {
        needsTemplateGuidance ? "template-unavailable" : creationFailureCode
    }

    private var needsTemplateGuidance: Bool {
        error.isEmpty && !working && creationState.permitsCreation
            && WindowsHVFProductPolicy.requiresTemplate(mode) && template == nil
    }
}

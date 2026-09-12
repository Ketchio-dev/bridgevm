import SwiftUI

extension CreateVMSheet {
    func completeCreation(_ config: VMConfig?) {
        working = false
        guard let config else { failCreation("vm-materialization-failed"); return }
        guard library.add(config) else { failCreation("library-publication-failed"); return }
        dismiss()
    }

    private func failCreation(_ code: String) {
        creationFailureCode = code
        error = "생성 또는 VM 라이브러리 저장 실패"
    }
}

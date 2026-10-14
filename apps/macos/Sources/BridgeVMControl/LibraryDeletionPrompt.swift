import Foundation

/// The library deletion confirmation dialog's wording. It names the bundle the
/// deletion removes, never a stale saved path the user would find empty.
struct LibraryDeletionPrompt: Equatable {
    let confirmTitle: String
    let message: String

    init(config: VMConfig, rootURL: URL) {
        let guidance = "실행 중인 VM은 삭제할 수 없습니다. 제어 화면에서 완전히 중지한 뒤 진행하세요."
        if let target = VMLibrary.deletionTarget(for: config, rootURL: rootURL) {
            confirmTitle = "\(config.name) 및 가상 디스크 삭제"
            message = "\(guidance) VM 설정과 \(target.path)에 있는 가상 디스크 데이터가 모두 영구 삭제되며 되돌릴 수 없습니다."
        } else {
            confirmTitle = "\(config.name) 등록 제거"
            message = "\(guidance) BridgeVM 라이브러리 등록만 제거하며 외부 번들 \(config.bundlePath)의 가상 디스크 데이터는 유지됩니다."
        }
    }
}

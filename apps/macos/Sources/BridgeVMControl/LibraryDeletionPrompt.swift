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

extension VMLibrary {
    /// The bundle deleting this VM removes, or nil when only the registration goes.
    /// Deletion (directly or behind the HVF media lease) is VMLibrary.delete, which
    /// recursively removes `<root>/<slug>/`; a stale saved path outside it is never
    /// what is deleted, but it must not hide the entry's own bundle.vmbridge.
    static func deletionTarget(for config: VMConfig, rootURL: URL = root) -> URL? {
        let rawEntry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        if (try? rawEntry.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true { return nil }
        let entry = rawEntry.resolvingSymlinksInPath().standardizedFileURL
        let saved = URL(fileURLWithPath: config.bundlePath)
        let bundle = saved.resolvingSymlinksInPath().standardizedFileURL
        let entryPrefix = entry.path.hasSuffix("/") ? entry.path : entry.path + "/"
        if bundle.path == entry.path || bundle.path.hasPrefix(entryPrefix) { return saved }
        let managed = rawEntry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        return (try? managed.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true ? managed : nil
    }
}

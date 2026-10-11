import Darwin
import Foundation

/// An unresolved install transaction keeps a journal bound to the current bundle
/// path. Launch and relocation both wait for it; nothing here rewrites the journal.
enum HvfWindowsInstallTransactionResidue {
    /// Clone copies the journal too, and the copy fails validation at its new path.
    static func refusal(_ verb: String) -> String {
        "Windows 설치 transaction 정리가 끝나지 않아 VM을 \(verb)하지 않았습니다. 설치를 다시 실행하지 말고 라이브러리를 새로고침한 뒤 다시 시도하세요. 계속 차단되면 보존된 transaction을 확인하세요."
    }

    /// Any residual entry or failed lookup counts. A missing journal inside a
    /// partly removed directory does not authorize launch or relocation.
    static func isPresent(_ config: VMConfig, rootURL: URL) -> Bool {
        let transaction = HvfWindowsInstallFinalization.paths(slug: config.slug, libraryRoot: rootURL,
                                                              bundlePath: config.bundlePath).transaction
        var info = stat()
        return lstat(transaction.path, &info) == 0 || errno != ENOENT
    }
}

extension LibraryModel {
    func transactionResidueRefusal(for cfg: VMConfig, action: LibraryFileAction) -> String? {
        guard action != .deletion, HvfWindowsInstallTransactionResidue.isPresent(cfg, rootURL: rootURL) else { return nil }
        return HvfWindowsInstallTransactionResidue.refusal(action == .move ? "이동" : "복제")
    }
}

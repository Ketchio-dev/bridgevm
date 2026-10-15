extension LibraryModel {
    func admitLibraryAction(_ cfg: VMConfig, action: LibraryFileAction) -> Bool {
        let generic = cloneSourceRefusal(for: cfg, action: action) ?? genericRetainedFileRefusal(for: cfg, action: action)
        let pending = action == .deletion ? pendingDeletionReferenceRefusal(for: cfg) : transactionResidueRefusal(for: cfg, action: action)
        guard let refusal = libraryActionRefusal(for: cfg) ?? generic ?? pending else { return true }
        switch action {
        case .deletion: deletionError = refusal
        case .clone: cloneError = refusal
        case .move: moveError = refusal
        }
        return false
    }
}

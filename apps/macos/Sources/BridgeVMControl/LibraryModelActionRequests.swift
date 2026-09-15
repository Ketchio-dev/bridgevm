extension LibraryModel {
    func requestDeletion(_ cfg: VMConfig) {
        guard !deletingSlugs.contains(cfg.slug) else { return }
        pendingDeletion = cfg
    }

    func requestWindowsClone(_ cfg: VMConfig) {
        guard cfg.engineKind == .hvfEngine,
              cfg.installPending != true,
              !cloningSlugs.contains(cfg.slug) else { return }
        pendingWindowsClone = cfg
    }
}

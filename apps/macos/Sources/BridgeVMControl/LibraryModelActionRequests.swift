extension LibraryModel {
    func requestDeletion(_ cfg: VMConfig) {
        guard admitLibraryAction(cfg, action: .deletion), !deletingSlugs.contains(cfg.slug) else { return }
        pendingDeletion = cfg
    }

    func requestWindowsClone(_ cfg: VMConfig) {
        guard cfg.engineKind == .hvfEngine,
              cfg.installPending != true,
              admitLibraryAction(cfg, action: .clone), !cloningSlugs.contains(cfg.slug) else { return }
        pendingWindowsClone = cfg
    }
}

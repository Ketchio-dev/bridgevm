extension LibraryModel {
    /// Publish a VMConfig that the create transaction has already persisted.
    /// Re-saving here would move persistence outside that transaction's rollback
    /// boundary and could report a false failure after a successful creation.
    @discardableResult
    func add(_ cfg: VMConfig) -> Bool {
        reload()
        guard vms.contains(where: { $0.slug == cfg.slug && $0.bundlePath == cfg.bundlePath }) else {
            return false
        }
        selectedID = cfg.slug
        return true
    }
}

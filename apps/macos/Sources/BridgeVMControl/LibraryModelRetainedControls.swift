import Combine

extension LibraryModel {
    var retainedControlRecords: [LibraryRetainedControlRecord] { retainedControlStore.records }

    var selectedRetainedControl: LibraryRetainedControlRecord? {
        retainedControlRecords.first { $0.id == selectedID }
    }

    func configureLibraryNavigation() {
        retainedControlStore.onChange = { [weak self] in self?.objectWillChange.send() }
        if selectedID == nil { selectedID = vms.first?.slug }
    }

    func reconcileLibrarySelection(configsBySlug: [String: VMConfig]) {
        if selectedRetainedControl != nil { return }
        if let sel = selectedID,
           ![Self.hvfEngineSelectionID, Self.firstRunImportSelectionID].contains(sel),
           configsBySlug[sel] == nil, let record = activeRetainedControl(for: sel) {
            selectedID = record.id
            return
        }
        if let sel = selectedID, ![Self.hvfEngineSelectionID, Self.firstRunImportSelectionID].contains(sel), configsBySlug[sel] == nil {
            selectedID = vms.first?.slug
        }
    }

    func captureRetainedControls() {
        let registered = Set(vms.map(\.slug))
        retainedControlStore.capture(
            windowsInstallSessions.retainedControls(excludingSlugs: registered)
                + hvfRuntimeSessions.retainedControls(excludingSlugs: registered))
    }

    private func activeRetainedControl(for slug: String) -> LibraryRetainedControlRecord? {
        retainedControlRecords
            .filter { $0.descriptor.config.slug == slug && $0.descriptor.isActive }
            .min { $0.descriptor.sortKey.lexicographicallyPrecedes($1.descriptor.sortKey) }
    }

    @discardableResult
    func dismissRetainedControl(_ token: String) -> Bool {
        guard retainedControlStore.dismiss(token) else { return false }
        if selectedID == token { selectedID = vms.first?.slug }
        return true
    }
}

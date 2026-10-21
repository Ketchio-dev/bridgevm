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

    private func activeRetainedControl(for slug: String) -> LibraryRetainedControlRecord? {
        retainedControlRecords
            .filter { $0.descriptor.config.slug == slug && $0.descriptor.isActive }
            .min { $0.descriptor.sortKey.lexicographicallyPrecedes($1.descriptor.sortKey) }
    }
}

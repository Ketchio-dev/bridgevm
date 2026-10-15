extension LibraryModel {
    func reload(revealing resultSlug: String, ifSelectionIs selectionAtAdmission: String?) {
        // Compare current identity before reconciliation, not navigation history.
        let revealResult = selectedID == selectionAtAdmission
        reload()
        if revealResult { selectedID = resultSlug }
    }
}

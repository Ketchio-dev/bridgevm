import Foundation

extension LibraryModel {
    @discardableResult
    func returnToFirstRunInputs() -> Bool {
        guard !firstRunImportBusy, firstRunImport.publishedConfig != nil else { return false }
        firstRunImport.returnToInputs()
        proMode = false
        selectedID = LibraryModel.firstRunImportSelectionID
        return true
    }
}

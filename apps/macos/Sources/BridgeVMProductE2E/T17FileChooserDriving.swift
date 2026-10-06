protocol T17FileChooserDriving {
    var failureContext: String { get }
    func open() throws
    func panelIsPresent() throws -> Bool
    func showLocationField() throws
    func locationFieldIsReady() throws -> Bool
    func setLocation(_ path: String) throws
    func acceptLocation() throws
    func locationFieldIsAbsent() throws -> Bool
    func acceptSelectionIfReady() throws -> Bool
    func selectedPath() throws -> String?
}

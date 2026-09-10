import Foundation
protocol T17FileChooserDriving {
    var failureContext: String { get }
    func open() throws
    func panelIsPresent() throws -> Bool
    func showLocationField() throws
    func locationFieldIsReady() throws -> Bool
    func setLocation(_ path: String) throws
    func acceptLocation() throws
    func locationFieldIsAbsent() throws -> Bool
    func selectionIsReady() throws -> Bool
    func acceptSelection() throws
    func selectedPath() throws -> String?
}

enum T17FileChooser {
    /// One deadline covers the whole interaction. An action returning success
    /// is not evidence that either the panel or the application accepted it.
    static func choose(
        path: String, timeout: TimeInterval, driver: T17FileChooserDriving,
        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
    ) throws {
        guard path.hasPrefix("/"), !path.contains("\0"), timeout.isFinite, timeout > 0 else {
            throw failure("invalid path or timeout")
        }
        let deadline = now() + timeout
        func wait(_ stage: String, until ready: () throws -> Bool) throws {
            repeat {
                if try ready() { return }
                if now() >= deadline { throw failure("timed out waiting for \(stage)" + diagnosticSuffix(driver)) }
                pause()
            } while true
        }
        guard try !driver.panelIsPresent() else { throw failure("a file chooser was already open") }
        try driver.open()
        try wait("file chooser") { try driver.panelIsPresent() }
        try driver.showLocationField()
        try wait("Go To location field") { try driver.locationFieldIsReady() }
        try driver.setLocation(path)
        try driver.acceptLocation()
        try wait("Go To sheet dismissal") { try driver.locationFieldIsAbsent() }
        try wait("enabled Open button") { try driver.selectionIsReady() }
        try driver.acceptSelection()
        try wait("chooser dismissal and exact selected path") {
            guard try !driver.panelIsPresent() else { return false }
            return try driver.selectedPath() == path
        }
    }

    static func failure(_ detail: String) -> T17Blocker {
        T17Blocker(code: "input-selection-failed", detail: detail)
    }
}

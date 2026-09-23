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
            try T17FileChooserWait.until(stage: stage, deadline: deadline,
                failureContext: { diagnosticSuffix(driver) }, now: now, pause: pause, ready: ready)
        }
        try T17FileChooserStage.run("initial-panel-check") { guard try !driver.panelIsPresent() else { throw failure("a file chooser was already open") } }
        try T17FileChooserStage.run("open-control") { try driver.open() }
        try T17FileChooserStage.run("panel-appearance") { try wait("file chooser") { try driver.panelIsPresent() } }
        try T17FileChooserStage.run("show-location-field") { try driver.showLocationField() }
        try T17FileChooserStage.run("location-field-ready") { try wait("Go To location field") { try driver.locationFieldIsReady() } }
        try T17FileChooserStage.run("set-location") { try driver.setLocation(path) }
        try T17FileChooserStage.run("accept-location") { try driver.acceptLocation() }
        try T17FileChooserStage.run("location-sheet-dismissal") { try wait("Go To sheet dismissal") { try driver.locationFieldIsAbsent() } }
        try T17FileChooserStage.run("selection-ready") { try wait("enabled Open button") { try driver.selectionIsReady() } }
        try T17FileChooserStage.run("accept-selection") { try driver.acceptSelection() }
        try T17FileChooserStage.run("selection-confirmation") { try wait("chooser dismissal and exact selected path") {
            guard try !driver.panelIsPresent() else { return false }
            return try driver.selectedPath() == path
        } }
    }
    static func failure(_ detail: String) -> T17Blocker {
        T17Blocker(code: "input-selection-failed", detail: detail)
    }
}

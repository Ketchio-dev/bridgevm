import Foundation
enum T17FileChooser {
    /// One deadline covers the whole interaction. An action returning success
    /// is not evidence that either the panel or the application accepted it.
    static func choose(
        path: String, timeout: TimeInterval, driver: T17FileChooserDriving,
        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
    ) throws {
        try choose(path: path, deadline: T17ChooserAdmission.deadline(timeout: timeout, now: now),
                   driver: driver, now: now, pause: pause)
    }
    static func choose(
        path: String, deadline: TimeInterval, driver: T17FileChooserDriving,
        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
    ) throws {
        guard path.hasPrefix("/"), !path.contains("\0"), deadline.isFinite else {
            throw failure("invalid path or timeout")
        }
        func wait(_ stage: String, until ready: () throws -> Bool) throws {
            try T17FileChooserWait.until(stage: stage, deadline: deadline,
                failureContext: { diagnosticSuffix(driver) }, now: now, pause: pause, ready: ready)
        }
        func run(_ stage: String, _ action: () throws -> Void) throws {
            try T17FileChooserDeadline.run(stage: stage, deadline: deadline,
                failureContext: { diagnosticSuffix(driver) }, now: now, action: action)
        }
        try run("initial-panel-check") { guard try !driver.panelIsPresent() else { throw failure("a file chooser was already open") } }
        try run("open-control") { try driver.open() }
        try run("panel-appearance") { try wait("file chooser") { try driver.panelIsPresent() } }
        try run("show-location-field") { try driver.showLocationField() }
        try run("location-field-ready") { try wait("Go To location field") { try driver.locationFieldIsReady() } }
        try run("set-location") { try driver.setLocation(path) }
        try run("accept-location") { try driver.acceptLocation() }
        try run("location-sheet-dismissal") { try wait("Go To sheet dismissal") { try driver.locationFieldIsAbsent() } }
        try run("selection-ready") { try wait("enabled Open button") { try driver.selectionIsReady() } }
        try run("accept-selection") { try driver.acceptSelection() }
        try run("selection-confirmation") { try wait("chooser dismissal and exact selected path") {
            guard try !driver.panelIsPresent() else { return false }
            return try driver.selectedPath() == path
        } }
    }
    static func failure(_ detail: String) -> T17Blocker {
        T17Blocker(code: "input-selection-failed", detail: detail)
    }
}

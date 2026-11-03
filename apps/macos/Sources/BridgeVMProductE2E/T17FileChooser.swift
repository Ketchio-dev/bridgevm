import Foundation
enum T17FileChooser {
    /// One deadline covers the whole interaction. An action returning success
    /// is not evidence that either the panel or the application accepted it.
    static func choose(
        path: String, deadline: TimeInterval, driver: T17FileChooserDriving, timing: T17ChooserTiming,
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
            try T17ChooserStageRunner.run(stage: stage, deadline: deadline, timing: timing,
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
        try run("accept-selection") { try wait("enabled Open button acceptance") { try driver.acceptSelectionIfReady() } }
        try run("selection-confirmation") { try wait("chooser dismissal and exact selected path") {
            guard try !driver.panelIsPresent() else { return false }
            return sameFile(try driver.selectedPath(), path)
        } }
    }
    /// The panel may report the resolved form of a symlinked request path (r96 on macOS 27),
    /// so the selection counts when it names the same file (device and inode), not only the same string.
    static func sameFile(_ selected: String?, _ path: String) -> Bool {
        guard let selected, !selected.isEmpty else { return false }
        if selected == path { return true }
        var chosen = stat(), requested = stat()
        guard selected.hasPrefix("/"), stat(selected, &chosen) == 0, stat(path, &requested) == 0 else { return false }
        return chosen.st_dev == requested.st_dev && chosen.st_ino == requested.st_ino
    }
    static func failure(_ detail: String) -> T17Blocker {
        T17Blocker(code: "input-selection-failed", detail: detail)
    }
}

import Foundation

/// The first-run import form is not a sheet, so its name field has no focus when the form
/// opens, and SwiftUI binds a value set through accessibility only while the field has focus.
/// T19 r1 and r2 set the name without focus and imported under the default "Windows 11"
/// (vTPM key account `windows-11`, 2026-10-22). Choosing the inputs updates the view, which
/// then shows the bound name; a name that did not persist stops the run before the commit.
enum A9ImportFormEntry {
    static let name = "bridgevm.first-run.name"

    static func enter(_ request: A9ImportRequest, ui: T17UIControlling) throws {
        try ui.press("bridgevm.first-run.import", timeout: 30)
        try ui.fill(request.vmName, identifier: name, timeout: 10)
        try ui.choose(path: request.sourceDiskPath, from: "bridgevm.first-run.disk.choose", timeout: 20)
        try ui.choose(path: request.sourceVarsPath, from: "bridgevm.first-run.vars.choose", timeout: 20)
        try ui.choose(path: request.sourceVtpmPath, from: "bridgevm.first-run.vtpm.choose", timeout: 20)
        try ui.choose(path: request.sourceVtpmPackagePath, from: "bridgevm.first-run.vtpm-package.choose", timeout: 20)
        try ui.choose(path: request.sourceVtpmCodePath, from: "bridgevm.first-run.vtpm-code.choose", timeout: 20)
        guard try ui.text(name, timeout: 10) == request.vmName else {
            throw T17Blocker(code: "input-selection-failed", detail: "import name field does not show the requested name after input selection")
        }
        try ui.press("bridgevm.first-run.import.commit", timeout: 15)
    }
}

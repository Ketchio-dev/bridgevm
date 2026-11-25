import SwiftUI

/// Keep an unsubmitted form in its owning window while SwiftUI rearranges the library.
struct FirstRunFormDraft {
    var displayName = "Windows 11"
    var diskPath = ""
    var varsPath = ""
    var vtpmPath = ""
    var vtpmPackagePath = ""
    var vtpmCodePath = ""
    var memGiB = CreateVMMemoryOptions.current.recommendedMiB / 1024
    var cpuCount = 4
}

private struct FirstRunFormDraftKey: EnvironmentKey {
    static let defaultValue: Binding<FirstRunFormDraft>? = nil
}

extension EnvironmentValues {
    var firstRunFormDraft: Binding<FirstRunFormDraft>? {
        get { self[FirstRunFormDraftKey.self] }
        set { self[FirstRunFormDraftKey.self] = newValue }
    }
}

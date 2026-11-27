import SwiftUI

/// The VM's "Print to Mac" setting (HvfGuestPrint.swift), saved in vm.json like the other per-VM settings and
/// changed only while the VM is stopped. Shown for library VMs, whose settings are saved.
struct HvfGuestPrintToggle: View {
    @ObservedObject var session: HvfEngineSession
    @State private var message: String?

    var body: some View {
        if session.persistsLibraryPreferences {
            Toggle("Print to Mac", isOn: Binding(get: { session.printToMacWanted }, set: update))
                .accessibilityIdentifier("bridgevm.runtime.print-to-mac")
                .disabled(!session.libraryPreferencesEditable)
                .help("On the next start, Windows adds a printer named Mac (BridgeVM). Print to it to open the Mac print dialog.")
                .alert("Printing settings", isPresented: Binding(get: { message != nil }, set: { if !$0 { message = nil } })) {
                    Button("OK") { message = nil }
                } message: {
                    Text(message ?? "")
                }
        }
    }

    private func update(_ enabled: Bool) {
        message = session.applyPrintToggle(enabled).message
    }
}

struct HvfGuestPrintNoticeView: View {
    @ObservedObject var notice: HvfGuestPrintNotice

    var body: some View {
        if let text = notice.text {
            Label(text, systemImage: "printer").font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("bridgevm.runtime.print-to-mac.notice")
        }
    }
}

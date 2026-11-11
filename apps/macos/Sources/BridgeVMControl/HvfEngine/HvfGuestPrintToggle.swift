import SwiftUI

/// The VM's "Mac으로 인쇄" setting (HvfGuestPrint.swift), saved in vm.json like the other per-VM settings and
/// changed only while the VM is stopped. Shown for library VMs, whose settings are saved.
struct HvfGuestPrintToggle: View {
    @ObservedObject var session: HvfEngineSession
    @State private var message: String?

    var body: some View {
        if session.persistsLibraryPreferences {
            Toggle("Mac으로 인쇄", isOn: Binding(get: { session.printToMacWanted }, set: update))
                .accessibilityIdentifier("bridgevm.runtime.print-to-mac")
                .disabled(!session.libraryPreferencesEditable)
                .help("켜면 Windows에 'Mac (BridgeVM)' 프린터가 생기고, 그 프린터로 인쇄한 문서가 Mac의 인쇄 창에서 열립니다. 다음 VM 시작부터 적용됩니다.")
                .alert("인쇄 설정", isPresented: Binding(get: { message != nil }, set: { if !$0 { message = nil } })) {
                    Button("확인") { message = nil }
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

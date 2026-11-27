import SwiftUI

/// The VM's "Open Windows links on Mac" setting (HvfGuestLinks.swift), saved in vm.json like the other per-VM
/// settings and changed only while the VM is stopped. Shown for library VMs, whose settings are saved.
struct HvfGuestLinksToggle: View {
    @ObservedObject var session: HvfEngineSession
    @State private var message: String?

    var body: some View {
        if session.persistsLibraryPreferences {
            Toggle("Open Windows links on Mac", isOn: Binding(get: { session.guestLinksWanted }, set: update))
                .accessibilityIdentifier("bridgevm.runtime.guest-links")
                .disabled(!session.libraryPreferencesEditable)
                .help("Starting next time, Windows web links can open in your default Mac browser. In Windows Settings, go to Apps > Default apps and choose BridgeVM as your browser.")
                .alert("Windows link settings", isPresented: Binding(get: { message != nil }, set: { if !$0 { message = nil } })) {
                    Button("OK") { message = nil }
                } message: {
                    Text(message ?? "")
                }
        }
    }

    private func update(_ enabled: Bool) {
        message = session.applyGuestLinksToggle(enabled).message
    }
}

/// The last Windows link the app did not open, by host only (a URL can carry a token). Cleared by the next
/// link it opens.
@MainActor
final class HvfGuestLinkNotice: ObservableObject {
    enum Kind: Equatable {
        case refused(host: String?), loopback(host: String), tooFrequent(host: String), failed(host: String)
    }

    @Published private(set) var shown: Kind?

    func show(_ kind: Kind?) {
        if shown != kind { shown = kind }
    }

    var text: String? {
        switch shown {
        case nil: return nil
        case let .refused(host?): return "Use an HTTP or HTTPS link to \(Self.short(host)) without a username or password."
        case .refused(nil): return "Windows sent an invalid link."
        case let .loopback(host): return "Open the link to \(Self.short(host)) in Windows. On your Mac, that address points back to the Mac itself."
        case let .tooFrequent(host): return "BridgeVM opens one Windows link per second. Try the link to \(Self.short(host)) again."
        case let .failed(host): return "Your Mac couldn’t open the Windows link to \(Self.short(host))."
        }
    }

    private static func short(_ host: String) -> String { host.count > 64 ? String(host.prefix(63)) + "…" : host }
}

struct HvfGuestLinkNoticeView: View {
    @ObservedObject var notice: HvfGuestLinkNotice

    var body: some View {
        if let text = notice.text {
            Label(text, systemImage: "link").font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("bridgevm.runtime.guest-links.notice")
        }
    }
}

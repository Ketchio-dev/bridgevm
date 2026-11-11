import SwiftUI

/// The VM's "Windows 링크를 Mac에서 열기" setting (HvfGuestLinks.swift), saved in vm.json like the other per-VM
/// settings and changed only while the VM is stopped. Shown for library VMs, whose settings are saved.
struct HvfGuestLinksToggle: View {
    @ObservedObject var session: HvfEngineSession
    @State private var message: String?

    var body: some View {
        if session.persistsLibraryPreferences {
            Toggle("Windows 링크를 Mac에서 열기", isOn: Binding(get: { session.guestLinksWanted }, set: update))
                .accessibilityIdentifier("bridgevm.runtime.guest-links")
                .disabled(!session.libraryPreferencesEditable)
                .help("켜면 Windows에서 연 http/https 링크가 Mac의 기본 브라우저에서 열립니다. 다음 VM 시작부터 적용되며, Windows 설정 > 앱 > 기본 앱에서 BridgeVM을 브라우저로 한 번 선택해야 합니다.")
                .alert("링크 설정", isPresented: Binding(get: { message != nil }, set: { if !$0 { message = nil } })) {
                    Button("확인") { message = nil }
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
        case let .refused(host?): return "Windows 링크(\(Self.short(host)))를 열지 않았습니다: http/https 주소가 아니거나 사용자 이름이 들어 있습니다."
        case .refused(nil): return "Windows에서 받은 링크가 올바르지 않아 열지 않았습니다."
        case let .loopback(host): return "Windows 링크(\(Self.short(host)))를 열지 않았습니다: Mac 자신을 가리키는 주소입니다."
        case let .tooFrequent(host): return "Windows 링크(\(Self.short(host)))를 열지 않았습니다: 링크는 1초에 하나까지 엽니다."
        case let .failed(host): return "Windows 링크(\(Self.short(host)))를 Mac에서 열지 못했습니다."
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

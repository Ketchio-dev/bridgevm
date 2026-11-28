import SwiftUI

/// A TCP port forward of the VM's network: the Mac's 127.0.0.1:`host` reaches the guest's `guest` port.
struct HvfPortForward: Equatable {
    var host: UInt16
    var guest: UInt16
}

enum HvfPortForwards {
    static let invalidMessage = "Enter a Mac port and a Windows port for each rule, separated by a colon. Separate rules with commas, for example 8080:80, 3390:3389."
    static let repeatedMessage = "Use each Mac port in only one rule: a Mac port can reach only one Windows port."
    static let reservedMessage = "Use Mac ports from 1024 up, for example 8080:80. macOS keeps lower ports for itself."

    /// macOS refuses an app's listener on 127.0.0.1 below 1024 (EACCES, checked on macOS 27); only
    /// 0.0.0.0 is allowed there, which would open the forward to the whole network.
    static let firstOpenMacPort: UInt16 = 1024

    /// Forwards that name a Mac port another rule already uses: only the first of them could listen.
    static func repeatsMacPort(_ forwards: [HvfPortForward]) -> Bool {
        Set(forwards.map(\.host)).count != forwards.count
    }

    /// The user's "8080:80, 3390:3389": nil when any entry is not two port numbers.
    static func parse(_ text: String) -> [HvfPortForward]? {
        var forwards: [HvfPortForward] = []
        for entry in text.split(separator: ",") {
            let trimmed = entry.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty { continue }
            let ports = trimmed.split(separator: ":", omittingEmptySubsequences: false)
            guard ports.count == 2,
                  let host = UInt16(ports[0].trimmingCharacters(in: .whitespaces)), host != 0,
                  let guest = UInt16(ports[1].trimmingCharacters(in: .whitespaces)), guest != 0
            else { return nil }
            forwards.append(HvfPortForward(host: host, guest: guest))
        }
        return forwards
    }

    /// vm.json's value, which can be hand-edited: an entry that does not parse is left out.
    static func saved(_ value: String?) -> [HvfPortForward] {
        (value ?? "").split(separator: ",").compactMap { parse(String($0))?.first }
    }

    static func text(_ forwards: [HvfPortForward]) -> String {
        forwards.map { "\($0.host):\($0.guest)" }.joined(separator: ", ")
    }
}

extension HvfEngineSession {
    /// While Windows runs with Guest Tools connected, forwards change at once: the VMM opens and closes its
    /// Mac ports (`BVNETFORWARD`, net_nat/port_forward.rs), and vm.json follows at the stop.
    var portForwardsLive: Bool { HvfHostSettingLiveSave.liveSlug(self) != nil && serviceStarted }

    /// vm.json's forwards for a library VM after the attempt (the visible text otherwise), plus any message.
    func applyPortForwards(_ text: String,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (text: String, message: String?) {
        guard let forwards = HvfPortForwards.parse(text) else { return (text, HvfPortForwards.invalidMessage) }
        guard !HvfPortForwards.repeatsMacPort(forwards) else { return (text, HvfPortForwards.repeatedMessage) }
        guard !forwards.contains(where: { $0.host < HvfPortForwards.firstOpenMacPort }) else {
            return (text, HvfPortForwards.reservedMessage)
        }
        guard persistsLibraryPreferences else {
            config.portForwards = forwards
            return (HvfPortForwards.text(forwards), nil)
        }
        if portForwardsLive, let slug = HvfHostSettingLiveSave.liveSlug(self) {
            let text = HvfPortForwards.text(forwards)
            guard sendCtl("BVNETFORWARD " + text) else {
                return (text, "BridgeVM couldn’t change the port forwards while Windows runs. Shut down Windows, then try again.")
            }
            HvfHostSettingLiveSave.record("Port forwarding", for: slug) { $0.portForwards = forwards.isEmpty ? nil : text }
            config.portForwards = forwards
            portForwardNotice.forwardsChanged()
            return (text, nil)
        }
        let message = saveLibraryPreference("Port forwarding", publish: publish) {
            $0.portForwards = forwards.isEmpty ? nil : HvfPortForwards.text(forwards)
        }
        return (HvfPortForwards.text(HvfPortForwards.saved(config.libraryContext?.config.portForwards)), message)
    }
}

/// Forwards the VMM could not open on the Mac (`BVNET forward-off host=<port> guest=<port>`,
/// net_nat/port_forward.rs forward_off_line), usually a port another app holds: the VM runs
/// without them, and until this notice nothing said so. Also whether Windows can reach the
/// Mac's DNS server (`BVNET dns-unreachable` / `dns-reachable`, net_nat/host_socket.rs).
/// Cleared at each run.
@MainActor
final class HvfPortForwardNotice: ObservableObject {
    @Published private(set) var off: [HvfPortForward] = []
    @Published private(set) var dnsUnreachable = false
    /// Windows ports whose forwarded connections Windows refused or never answered, until one opens
    /// (`BVNET forward-unanswered guest=<port>` / `forward-answered`).
    @Published private(set) var unanswered: [UInt16] = []
    static let dnsUnreachableText = "Windows can’t reach your Mac’s DNS server, so websites won’t open. If BridgeVM is off in System Settings > Privacy & Security > Local Network, turn it on."

    func consume(_ line: String) {
        let line = line.hasSuffix("\r") ? String(line.dropLast()) : line
        switch line {
        case "BVNET dns-unreachable": if !dnsUnreachable { dnsUnreachable = true }; return
        case "BVNET dns-reachable": if dnsUnreachable { dnsUnreachable = false }; return
        default: break
        }
        for (prefix, answered) in [("BVNET forward-unanswered guest=", false), ("BVNET forward-answered guest=", true)]
            where line.hasPrefix(prefix) {
            guard let port = UInt16(line.dropFirst(prefix.count)) else { return }
            if answered { unanswered.removeAll { $0 == port } } else if !unanswered.contains(port) { unanswered.append(port) }
            return
        }
        let prefix = "BVNET forward-off host="
        guard line.hasPrefix(prefix) else { return }
        let parts = line.dropFirst(prefix.count).split(separator: " ")
        guard parts.count == 2, let host = UInt16(parts[0]), parts[1].hasPrefix("guest="),
              let guest = UInt16(parts[1].dropFirst("guest=".count)) else { return }
        let forward = HvfPortForward(host: host, guest: guest)
        if !off.contains(forward) { off.append(forward) }
    }

    /// New forwards: the VMM says again which of them it couldn't open.
    func forwardsChanged() {
        if !off.isEmpty { off = [] }
        if !unanswered.isEmpty { unanswered = [] }
    }

    func reset() {
        if !off.isEmpty { off = [] }
        if dnsUnreachable { dnsUnreachable = false }
        if !unanswered.isEmpty { unanswered = [] }
    }

    var text: String? {
        let parts = [dnsUnreachable ? Self.dnsUnreachableText : nil, forwardsText, unansweredText].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " ")
    }

    private var unansweredText: String? {
        guard !unanswered.isEmpty else { return nil }
        let ports = unanswered.map(String.init).joined(separator: ", ")
        let port = unanswered.count == 1 ? "port \(ports)" : "ports \(ports)"
        return "Windows isn’t accepting forwarded connections on \(port). Start the app that should listen there, or allow it in Windows Defender Firewall."
    }

    private var forwardsText: String? {
        guard let first = off.first else { return nil }
        // A hand-edited vm.json can still hold one; no other app is involved.
        if let reserved = off.first(where: { $0.host < HvfPortForwards.firstOpenMacPort }) {
            return "Mac port \(reserved.host) doesn’t reach Windows port \(reserved.guest) because macOS keeps ports below 1024 for itself. Choose a Mac port from 1024 up in Settings, then restart Windows."
        }
        if off.count == 1 {
            return "Another app or another BridgeVM VM is using Mac port \(first.host), so it doesn’t reach Windows port \(first.guest). Quit that app or shut down that VM, or choose another Mac port in Settings, then restart Windows."
        }
        let ports = off.map { String($0.host) }.joined(separator: ", ")
        return "Other apps or other BridgeVM VMs are using Mac ports \(ports), so they don’t reach Windows. Quit those apps or shut down those VMs, or choose other Mac ports in Settings, then restart Windows."
    }
}

struct HvfPortForwardNoticeView: View {
    @ObservedObject var notice: HvfPortForwardNotice

    var body: some View {
        if let text = notice.text {
            Label(text, systemImage: "network.slash").font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("bridgevm.runtime.port-forward.notice")
        }
    }
}

/// The runtime view's port forwards. For a library VM, Return and Start save them in vm.json.
struct HvfPortForwardSetting: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var text: String
    @State private var message: String?
    var commit: (() -> Void)? = nil
    @FocusState private var focused: Bool

    var isDisabled: Bool {
        session.persistsLibraryPreferences && !session.libraryPreferencesEditable && !session.portForwardsLive
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("Port forwards").frame(width: 92, alignment: .leading)
                TextField("", text: $text, prompt: Text("8080:80, 3390:3389"))
                    .labelsHidden()
                    .accessibilityLabel("Port forwards")
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.portForwards")
                    .focused($focused)
                    .onSubmit(commitDraft)
            }
            Text(isDisabled && session.hasActiveRuntimeWork
                 ? "Shut down Windows to change port forwards, or install Guest Tools to change them while Windows runs."
                 : "Enter Mac port:Windows port pairs, separated by commas. Example: 8080:80.")
                .font(.caption).foregroundColor(.secondary)
        }
        .disabled(isDisabled)
        .onChange(of: focused) { _, active in if !active { commitDraft() } }
        .help("Connect to 127.0.0.1 on the Mac port to reach the Windows TCP port.")
        .alert("Port forwarding", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("OK") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    private func commitDraft() {
        if let commit { commit(); return }
        let result = session.applyPortForwards(text)
        text = result.text
        message = result.message
    }
}

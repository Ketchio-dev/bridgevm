import SwiftUI

/// A TCP port forward of the VM's network: the Mac's 127.0.0.1:`host` reaches the guest's `guest` port.
struct HvfPortForward: Equatable {
    var host: UInt16
    var guest: UInt16
}

enum HvfPortForwards {
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
    /// vm.json's forwards for a library VM after the attempt (the visible text otherwise), plus any message.
    func applyPortForwards(_ text: String,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (text: String, message: String?) {
        guard persistsLibraryPreferences else { return (text, nil) }
        guard let forwards = HvfPortForwards.parse(text) else {
            return (text, "포트 포워딩은 Mac포트:Windows포트 형식으로 쉼표로 구분해 입력하세요. 예: 8080:80, 3390:3389")
        }
        let message = saveLibraryPreference("포트 포워딩 설정", publish: publish) {
            $0.portForwards = forwards.isEmpty ? nil : HvfPortForwards.text(forwards)
        }
        return (HvfPortForwards.text(HvfPortForwards.saved(config.libraryContext?.config.portForwards)), message)
    }

    /// Runtime-view Start launches what vm.json holds, so typed but unsaved forwards are saved
    /// first. Forwards never stop the VM from starting: when they cannot be saved, it starts with
    /// the saved ones. Returns the forwards it starts with and a notice for the user, if any.
    func savePortForwardsBeforeStart(_ visible: String) -> (text: String, notice: String?) {
        guard persistsLibraryPreferences, let saved = config.libraryContext?.config else { return (visible, nil) }
        let savedText = HvfPortForwards.text(HvfPortForwards.saved(saved.portForwards))
        guard HvfPortForwards.parse(visible) != HvfPortForwards.saved(saved.portForwards) else { return (savedText, nil) }
        let result = applyPortForwards(visible)
        guard result.message != nil else { return (result.text, nil) }
        let started = HvfPortForwards.text(HvfPortForwards.saved(config.libraryContext?.config.portForwards))
        let using = started.isEmpty ? "포트 포워딩 없이" : "저장된 포트 포워딩(\(started))으로"
        return (started, "입력한 포트 포워딩을 저장하지 못해 \(using) 시작합니다. \(result.message ?? "")")
    }
}

/// The runtime view's port forwards. For a library VM, Return and Start save them in vm.json.
struct HvfPortForwardSetting: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var text: String
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        HStack {
            Text("Port forwards").frame(width: 92, alignment: .leading)
            TextField("8080:80, 3390:3389", text: $text)
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
                .accessibilityIdentifier("bridgevm.runtime.portForwards")
                .onSubmit(apply)
        }
        .disabled(isDisabled)
        .help("Mac의 127.0.0.1:Mac포트 연결이 Windows의 TCP 포트로 전달됩니다. VM이 중지된 동안 변경하세요.")
        .alert("포트 포워딩 설정", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("확인") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    private func apply() {
        let result = session.applyPortForwards(text)
        text = result.text
        message = result.message
    }
}

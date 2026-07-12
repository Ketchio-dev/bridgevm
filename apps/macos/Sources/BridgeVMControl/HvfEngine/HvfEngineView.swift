import SwiftUI
#if canImport(AppKit)
import AppKit
#endif

struct HvfEngineView: View {
    @StateObject private var session: HvfEngineSession
    @State private var targetDiskPath = ""
    @State private var uefiVarsPath = ""
    @State private var evidenceDir = ""
    @State private var watchdogEnabled = false
    @State private var watchdogMs = 900_000
    @State private var ramMiB = 6144
    @State private var smpCpus = 4
    @State private var clipboardSync = true
    @State private var shareEnabled = false
    @State private var shareHostDir = ""
    @State private var shareGuestDir = "C:\\bridgevm-share"
    @State private var virtioNet = false
    @State private var virtioGpu3d = true
    @State private var nvmeBufferedIO = true
    @State private var ctlFilePath = ""
    @State private var ctlInput = ""
    @State private var keyboardInput = ""
    @State private var pointerDragging = false
    @State private var lastPointerLocation: CGPoint?
    @FocusState private var displayFocused: Bool

    init(config: HvfEngineConfig = HvfEngineView.defaultConfig()) {
        _session = StateObject(wrappedValue: HvfEngineSession(config: config))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                configCard
                statusCard
                screenshotCard
                eventFeedCard
            }
            .padding(20)
        }
        .navigationTitle("HVF Engine")
        .onAppear {
            loadStateFromSession()
            session.attachToRunningVM()
        }
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text("HVF Engine (Experimental)").font(.largeTitle.bold())
                Text("Windows 11 ARM64 from-scratch HVF backend").foregroundColor(.secondary)
            }
            Spacer()
            statusPill
        }
    }

    private var configCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                pathRow("Target disk", text: $targetDiskPath, chooseDirectory: false)
                pathRow("UEFI vars", text: $uefiVarsPath, chooseDirectory: false)
                pathRow("Evidence dir", text: $evidenceDir, chooseDirectory: true)
                pathRow("CTL file", text: $ctlFilePath, chooseDirectory: false)
                HStack {
                    Text("Watchdog").frame(width: 92, alignment: .leading)
                    Toggle("Enabled", isOn: $watchdogEnabled)
                        .toggleStyle(.checkbox)
                    Stepper("\(watchdogMs) ms", value: $watchdogMs, in: 60_000...86_400_000, step: 30_000)
                        .font(.body.monospaced())
                        .disabled(!watchdogEnabled)
                    Spacer()
                }
                HStack(spacing: 24) {
                    Stepper("RAM \(ramMiB) MiB", value: $ramMiB, in: 1024...65_536, step: 1024)
                        .font(.body.monospaced())
                    Stepper("CPU \(smpCpus)", value: $smpCpus, in: 1...123)
                        .font(.body.monospaced())
                    Spacer()
                }
                HStack(spacing: 18) {
                    Toggle("Clipboard sync", isOn: $clipboardSync)
                    Toggle("Virtio net", isOn: $virtioNet)
                    Toggle("VirGL 3D", isOn: $virtioGpu3d)
                    Toggle("Buffered NVMe", isOn: $nvmeBufferedIO)
                    Toggle("Shared folder", isOn: $shareEnabled)
                    Spacer()
                }
                if shareEnabled {
                    pathRow("Host share", text: $shareHostDir, chooseDirectory: true)
                    HStack {
                        Text("Guest share").frame(width: 92, alignment: .leading)
                        TextField("C:\\bridgevm-share", text: $shareGuestDir)
                            .textFieldStyle(.roundedBorder)
                            .font(.body.monospaced())
                    }
                }
                HStack(spacing: 8) {
                    TextField("Type text into Windows", text: $keyboardInput, onCommit: sendKeyboardText)
                        .textFieldStyle(.roundedBorder)
                    Button("Type", action: sendKeyboardText)
                    Button("Tab") { session.sendKey("tab") }
                    Button("Enter") { session.sendKey("enter") }
                    Button("Space") { session.sendKey("space") }
                }
                HStack(spacing: 8) {
                    Button("Esc") { session.sendKey("esc") }
                    Button("⌫") { session.sendKey("backspace") }.help("Backspace")
                    Button("⌦") { session.sendKey("delete") }.help("Delete")
                    Divider().frame(height: 18)
                    Button("←") { session.sendKey("left") }
                    Button("↑") { session.sendKey("up") }
                    Button("↓") { session.sendKey("down") }
                    Button("→") { session.sendKey("right") }
                    Button("Home") { session.sendKey("home") }
                    Button("End") { session.sendKey("end") }
                    Spacer()
                    Button("Ctrl-Alt-Delete") { session.sendKey("ctrl+alt+delete") }
                }
            }
            .padding(6)
        } label: {
            Label("Boot Configuration", systemImage: "gearshape.2")
        }
    }

    private var statusCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    Button(action: start) { Label("Start", systemImage: "play.fill") }
                        .disabled(!bootConfigReady)
                    Button(action: session.stop) { Label("Stop", systemImage: "stop.fill") }
                    Button(action: sendCtl) { Label("Send", systemImage: "paperplane.fill") }
                        .disabled(ctlInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    TextField("CLIPGET, CLIPSET ..., or guest shell command", text: $ctlInput, onCommit: sendCtl)
                        .textFieldStyle(.roundedBorder)
                        .font(.body.monospaced())
                }
                HStack(spacing: 24) {
                    infoItem("State", stateText)
                    infoItem("Heartbeat", heartbeatText)
                    infoItem("Events", "\(session.events.count)")
                }
            }
            .padding(6)
        } label: {
            Label("Control Channel", systemImage: "terminal")
        }
    }

    private var screenshotCard: some View {
        GroupBox {
            #if canImport(AppKit)
            if let image = session.latestScreenshot {
                GeometryReader { geometry in
                    Image(nsImage: image)
                        .resizable()
                        .interpolation(.none)
                        .scaledToFit()
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .contentShape(Rectangle())
                        .focusable()
                        .focused($displayFocused)
                        .onKeyPress { press in
                            handleDisplayKeyPress(press)
                        }
                        .gesture(
                            DragGesture(minimumDistance: 0)
                                .onChanged { value in
                                    displayFocused = true
                                    lastPointerLocation = value.location
                                    if pointerDragging {
                                        session.sendPointerMove(
                                            location: value.location,
                                            viewSize: geometry.size,
                                            imageSize: image.size
                                        )
                                    } else {
                                        pointerDragging = true
                                        session.sendPointerPress(
                                            location: value.location,
                                            viewSize: geometry.size,
                                            imageSize: image.size
                                        )
                                    }
                                }
                                .onEnded { value in
                                    displayFocused = true
                                    lastPointerLocation = value.location
                                    session.sendPointerRelease(
                                        location: value.location,
                                        viewSize: geometry.size,
                                        imageSize: image.size
                                    )
                                    pointerDragging = false
                                }
                        )
                        .overlay(alignment: .bottomTrailing) {
                            HStack(spacing: 6) {
                                Button("Right Click") {
                                    let location = lastPointerLocation ?? CGPoint(
                                        x: geometry.size.width / 2,
                                        y: geometry.size.height / 2
                                    )
                                    session.sendPointerRightClick(
                                        location: location,
                                        viewSize: geometry.size,
                                        imageSize: image.size
                                    )
                                }
                                Button("Scroll Up") {
                                    sendDisplayScroll(1, geometry: geometry, image: image)
                                }
                                Button("Scroll Down") {
                                    sendDisplayScroll(-1, geometry: geometry, image: image)
                                }
                            }
                            .buttonStyle(.bordered)
                            .padding(8)
                        }
                }
                .frame(maxWidth: .infinity, minHeight: 280, maxHeight: 520)
                .background(Color.black)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(displayFocused ? Color.accentColor : Color.clear, lineWidth: 2)
                )
                .cornerRadius(6)
            } else {
                VStack(spacing: 8) {
                    Image(systemName: "display").font(.system(size: 36)).foregroundColor(.secondary)
                    Text("Waiting for the live display").foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 280)
            }
            #else
            Text("Screenshots require AppKit").foregroundColor(.secondary)
            #endif
        } label: {
            Label("Live Display", systemImage: "display")
        }
    }

    private var eventFeedCard: some View {
        GroupBox {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(session.events.enumerated()), id: \.offset) { _, event in
                        Text(event.displayText)
                            .font(.system(size: 11, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                    }
                }
                .padding(8)
            }
            .frame(minHeight: 220)
            .background(Color(white: 0.1))
            .foregroundColor(Color(white: 0.9))
            .cornerRadius(6)
        } label: {
            Label("BVAGENT Event Feed", systemImage: "list.bullet.rectangle")
        }
    }

    #if canImport(AppKit)
    private func sendDisplayScroll(_ delta: Int8, geometry: GeometryProxy, image: NSImage) {
        let location = lastPointerLocation ?? CGPoint(
            x: geometry.size.width / 2,
            y: geometry.size.height / 2
        )
        session.sendPointerScroll(
            delta,
            location: location,
            viewSize: geometry.size,
            imageSize: image.size
        )
    }
    #endif

    private var statusPill: some View {
        Text(stateText)
            .font(.callout)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(stateColor.opacity(0.18))
            .foregroundColor(stateColor)
            .cornerRadius(8)
    }

    private var stateText: String {
        switch session.connectionState {
        case .stopped: return "Stopped"
        case .booting: return "Booting"
        case let .connected(host): return "Connected: \(host)"
        case .stopping: return "Stopping"
        case .timedOut: return "Timed out"
        }
    }

    private var stateColor: Color {
        switch session.connectionState {
        case .stopped: return .secondary
        case .booting: return .orange
        case .connected: return .green
        case .stopping: return .orange
        case .timedOut: return .red
        }
    }

    private var heartbeatText: String {
        guard let age = session.lastHeartbeatAge else { return "-" }
        return String(format: "%.1fs ago", age)
    }

    private var bootConfigReady: Bool {
        guard !targetDiskPath.isEmpty, !uefiVarsPath.isEmpty,
              !evidenceDir.isEmpty, !ctlFilePath.isEmpty,
              FileManager.default.fileExists(atPath: targetDiskPath),
              FileManager.default.fileExists(atPath: uefiVarsPath) else { return false }
        if shareEnabled {
            var isDirectory: ObjCBool = false
            guard !shareGuestDir.isEmpty,
                  FileManager.default.fileExists(atPath: shareHostDir, isDirectory: &isDirectory),
                  isDirectory.boolValue else { return false }
        }
        return true
    }

    private func pathRow(_ label: String, text: Binding<String>, chooseDirectory: Bool) -> some View {
        HStack {
            Text(label).frame(width: 92, alignment: .leading)
            TextField(label, text: text)
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
            Button { choosePath(text: text, directory: chooseDirectory) } label: {
                Image(systemName: "folder")
            }
            .help("Choose \(label)")
        }
    }

    private func infoItem(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundColor(.secondary)
            Text(value).font(.body.monospaced())
        }
    }

    private func start() {
        session.config = currentConfig()
        session.start()
    }

    private func sendCtl() {
        session.sendCtl(ctlInput)
        ctlInput = ""
    }

    private func sendKeyboardText() {
        session.sendText(keyboardInput)
        keyboardInput = ""
    }

    private func handleDisplayKeyPress(_ press: KeyPress) -> KeyPress.Result {
        switch HvfHostKeyCommand.resolve(characters: press.characters, modifiers: press.modifiers) {
        case let .key(action):
            session.sendKey(action)
            return .handled
        case let .text(text):
            session.sendText(text)
            return .handled
        case .ignored:
            return .ignored
        }
    }

    private func currentConfig() -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: targetDiskPath,
                        uefiVarsPath: uefiVarsPath,
                        evidenceDir: evidenceDir,
                        watchdogMs: watchdogEnabled ? watchdogMs : nil,
                        ramMiB: ramMiB,
                        smpCpus: smpCpus,
                        clipboardSync: clipboardSync,
                        shareHostDir: shareEnabled ? shareHostDir : nil,
                        shareGuestDir: shareEnabled ? shareGuestDir : nil,
                        virtioNet: virtioNet,
                        virtioGpu3d: virtioGpu3d,
                        nvmeBufferedIO: nvmeBufferedIO,
                        ctlFilePath: ctlFilePath)
    }

    private func loadStateFromSession() {
        let cfg = session.config
        targetDiskPath = cfg.targetDiskPath
        uefiVarsPath = cfg.uefiVarsPath
        evidenceDir = cfg.evidenceDir
        watchdogEnabled = cfg.watchdogMs != nil
        watchdogMs = cfg.watchdogMs ?? 900_000
        ramMiB = cfg.ramMiB
        smpCpus = cfg.smpCpus
        clipboardSync = cfg.clipboardSync
        shareEnabled = cfg.shareHostDir != nil && cfg.shareGuestDir != nil
        shareHostDir = cfg.shareHostDir ?? ""
        shareGuestDir = cfg.shareGuestDir ?? "C:\\bridgevm-share"
        virtioNet = cfg.virtioNet
        virtioGpu3d = cfg.virtioGpu3d
        nvmeBufferedIO = cfg.nvmeBufferedIO
        ctlFilePath = cfg.ctlFilePath
    }

    private func choosePath(text: Binding<String>, directory: Bool) {
        #if canImport(AppKit)
        let panel = NSOpenPanel()
        panel.canChooseFiles = !directory
        panel.canChooseDirectories = directory
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            text.wrappedValue = url.path
        }
        #endif
    }

    static func defaultConfig() -> HvfEngineConfig {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let evidence = home.appendingPathComponent("BridgeVM/HVF/evidence", isDirectory: true).path
        return HvfEngineConfig(targetDiskPath: "",
                               uefiVarsPath: "",
                               evidenceDir: evidence,
                               watchdogMs: nil,
                               ramMiB: 6144,
                               smpCpus: 4,
                               clipboardSync: true,
                               shareHostDir: nil,
                               shareGuestDir: nil,
                               virtioNet: false,
                               virtioGpu3d: true,
                               nvmeBufferedIO: true,
                               ctlFilePath: "\(evidence)/bvagent.ctl")
    }
}

enum HvfHostKeyCommand: Equatable {
    case key(String)
    case text(String)
    case ignored

    static func resolve(characters: String, modifiers: EventModifiers = []) -> HvfHostKeyCommand {
        if characters == "\u{7f}", modifiers.contains(.control), modifiers.contains(.option) {
            return .key("ctrl+alt+delete")
        }
        switch characters {
        case "\u{1b}": return .key("esc")
        case "\u{7f}": return .key("backspace")
        case "\u{f728}": return .key("delete")
        case "\u{f700}": return .key("up")
        case "\u{f701}": return .key("down")
        case "\u{f702}": return .key("left")
        case "\u{f703}": return .key("right")
        case "\u{f729}": return .key("home")
        case "\u{f72b}": return .key("end")
        case "\u{f72c}": return .key("pageup")
        case "\u{f72d}": return .key("pagedown")
        case "\t": return .key("tab")
        case "\r", "\n": return .key("enter")
        default:
            guard !characters.isEmpty,
                  !modifiers.contains(.command),
                  !modifiers.contains(.control),
                  !modifiers.contains(.option) else { return .ignored }
            return .text(characters)
        }
    }
}

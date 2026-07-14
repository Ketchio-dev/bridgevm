#if canImport(AppKit)
import AppKit
import SwiftUI

struct HvfLiveDisplaySurface: View {
    @ObservedObject var session: HvfEngineSession
    @FocusState private var displayFocused: Bool

    var body: some View {
        GeometryReader { geometry in
            if let image = session.latestScreenshot {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .contentShape(Rectangle())
                    .focusable()
                    .focused($displayFocused)
                    .onKeyPress { press in
                        handleDisplayKeyPress(press)
                    }
                    .overlay {
                        HvfPointerEventSurface(
                            onFocus: { displayFocused = true },
                            onMove: { location in
                                session.sendPointerMove(
                                    location: location,
                                    viewSize: geometry.size,
                                    imageSize: image.size
                                )
                            },
                            onLeftDown: { location in
                                session.sendPointerPress(
                                    location: location,
                                    viewSize: geometry.size,
                                    imageSize: image.size
                                )
                            },
                            onLeftUp: { location in
                                session.sendPointerRelease(
                                    location: location,
                                    viewSize: geometry.size,
                                    imageSize: image.size
                                )
                            },
                            onRightDown: { location in
                                session.sendPointerRightPress(
                                    location: location,
                                    viewSize: geometry.size,
                                    imageSize: image.size
                                )
                            },
                            onRightUp: { location in
                                session.sendPointerRelease(
                                    location: location,
                                    viewSize: geometry.size,
                                    imageSize: image.size
                                )
                            },
                            onScroll: { delta, location in
                                session.sendPointerScroll(
                                    delta,
                                    location: location,
                                    viewSize: geometry.size,
                                    imageSize: image.size
                                )
                            }
                        )
                    }
                    .overlay {
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(displayFocused ? Color.accentColor : Color.clear, lineWidth: 2)
                            .allowsHitTesting(false)
                    }
            } else {
                VStack(spacing: 10) {
                    Image(systemName: "display")
                        .font(.system(size: 42))
                    Text("라이브 디스플레이를 기다리는 중입니다.")
                }
                .foregroundStyle(.secondary)
                .frame(width: geometry.size.width, height: geometry.size.height)
            }
        }
        .background(Color.black)
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
}

@MainActor
final class HvfDisplayWindowController: NSWindowController, NSWindowDelegate {
    private static var controllers: [ObjectIdentifier: HvfDisplayWindowController] = [:]

    private let sessionIdentifier: ObjectIdentifier

    private init(window: NSWindow, sessionIdentifier: ObjectIdentifier) {
        self.sessionIdentifier = sessionIdentifier
        super.init(window: window)
        window.delegate = self
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    static func present(session: HvfEngineSession, title: String) {
        let identifier = ObjectIdentifier(session)

        if let controller = controllers[identifier], let window = controller.window {
            window.title = title
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }

        let contentRect = NSRect(origin: .zero, size: NSSize(width: 1280, height: 800))
        let window = NSWindow(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = title
        let hostingController = NSHostingController(
            rootView: HvfLiveDisplaySurface(session: session)
                .frame(minWidth: 640, minHeight: 400)
        )
        if #available(macOS 13.0, *) {
            hostingController.sizingOptions = []
        }
        window.contentViewController = hostingController
        window.setContentSize(NSSize(width: 1280, height: 800))
        window.center()
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.backgroundColor = .black
        window.isReleasedWhenClosed = false

        let controller = HvfDisplayWindowController(
            window: window,
            sessionIdentifier: identifier
        )
        controllers[identifier] = controller

        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        if HvfDisplayWindowController.controllers[sessionIdentifier] === self {
            HvfDisplayWindowController.controllers[sessionIdentifier] = nil
        }
    }
}
#endif

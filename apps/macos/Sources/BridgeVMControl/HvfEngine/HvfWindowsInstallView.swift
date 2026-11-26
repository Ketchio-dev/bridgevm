import SwiftUI

/// Detail panel for a Windows HVF VM whose unattended install has not run
/// yet: shows the persisted install request, drives the scripted-install
/// pipeline, and flips the VM to bootable when the media lands in the bundle.
struct HvfWindowsInstallView: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel
    @ObservedObject private(set) var session: HvfWindowsInstallSession
    @State private var showsLog = false

    init(config: VMConfig, library: LibraryModel, session: HvfWindowsInstallSession) {
        self.config = config
        self.library = library
        _session = ObservedObject(wrappedValue: session)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Install Windows").font(.title.bold())
                    Text(config.displayName).font(.headline).foregroundStyle(.secondary)
                }
                HvfWindowsInstallStatusCard(session: session, installerForNewVM: installerForNewVM)
                if case .failed = session.stage, !session.logLines.isEmpty {
                    logCard(showsLog ? "Hide error log" : "Show error log")
                    requestCard
                } else {
                    requestCard
                    if !session.logLines.isEmpty { logCard("Installation log") }
                }
            }
            .padding(20)
            .frame(maxWidth: 760, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(LibraryAppearance.canvas)
        .accessibilityIdentifier("bridgevm.windows.install.view")
    }

    var installerForNewVM: URL? {
        let request = session.plan.request
        guard session.stage == .idle, request.isoSHA256 != nil,
              request.isoPath == config.bundlePath + "/" + WindowsHVFProductPolicy.installerISO,
              request.guestPayloadDirectory == nil, request.guestPayloadManifest == nil,
              request.guestPayloadIdentity == nil,
              !HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: library.rootURL) else { return nil }
        return URL(fileURLWithPath: request.isoPath)
    }

    private var requestCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Installation settings").font(.headline)
            row("ISO") { HvfWindowsInstallISOButton(path: session.plan.request.isoPath) }
            row("Disk") { Text("\(session.plan.request.diskGiB) GiB") }
            row("Graphics") {
                Text(session.plan.request.injectViogpu3d ? "3D acceleration is unavailable" : "Standard display · 3D off")
            }
            if session.plan.sourceImageCacheCandidateExists {
                row("Install media") { Text("Cached media available. BridgeVM verifies it at startup.") }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .modifier(LibraryCardSurface())
    }

    private func logCard(_ title: String) -> some View {
        DisclosureGroup(isExpanded: $showsLog) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(Array(session.logLines.enumerated()), id: \.offset) { index, line in
                            Text(line)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(.secondary)
                                .textSelection(.enabled)
                                .id(index)
                        }
                    }
                }
                .frame(height: 220)
                .onAppear {
                    if !session.logLines.isEmpty { proxy.scrollTo(session.logLines.count - 1, anchor: .bottom) }
                }
                .onChange(of: session.logLines) { _, lines in
                    if !lines.isEmpty { proxy.scrollTo(lines.count - 1, anchor: .bottom) }
                }
            }
        } label: {
            Label(title, systemImage: "text.alignleft").font(.headline)
        }
        .accessibilityIdentifier("bridgevm.windows.install.log")
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .modifier(LibraryCardSurface())
    }

    private func row<Content: View>(_ label: String, @ViewBuilder content: () -> Content) -> some View {
        HStack {
            Text(label).frame(width: 92, alignment: .leading).foregroundColor(.secondary)
            content().lineLimit(1).truncationMode(.middle)
        }
        .font(.callout)
    }

}

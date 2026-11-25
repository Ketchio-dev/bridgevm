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
                    Text("Windows 설치").font(.title.bold())
                    Text(config.displayName).font(.headline).foregroundStyle(.secondary)
                }
                HvfWindowsInstallStatusCard(session: session)
                requestCard
                if !session.logLines.isEmpty { logCard }
            }
            .padding(20)
            .frame(maxWidth: 760, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(LibraryAppearance.canvas)
        .accessibilityIdentifier("bridgevm.windows.install.view")
    }

    private var requestCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("설치 구성").font(.headline)
            row("ISO", (session.plan.request.isoPath as NSString).lastPathComponent)
            row("디스크", "\(session.plan.request.diskGiB) GiB")
            row("그래픽", session.plan.request.injectViogpu3d
                ? "차단됨 — 3D 가속 미지원" : "기본 디스플레이 · 3D 제외")
            if session.plan.sourceImageCacheCandidateExists {
                row("설치 소스", "저장된 소스 있음 · 시작 시 확인")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .modifier(LibraryCardSurface())
    }

    private var logCard: some View {
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
            Label("설치 로그", systemImage: "text.alignleft").font(.headline)
        }
        .accessibilityIdentifier("bridgevm.windows.install.log")
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .modifier(LibraryCardSurface())
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).frame(width: 92, alignment: .leading).foregroundColor(.secondary)
            Text(value).lineLimit(1).truncationMode(.middle)
        }
        .font(.callout)
    }

}

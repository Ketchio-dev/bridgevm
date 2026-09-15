import SwiftUI

/// Detail panel for a Windows HVF VM whose unattended install has not run
/// yet: shows the persisted install request, drives the scripted-install
/// pipeline, and flips the VM to bootable when the media lands in the bundle.
struct HvfWindowsInstallView: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel
    @ObservedObject private(set) var session: HvfWindowsInstallSession

    init(config: VMConfig, library: LibraryModel, session: HvfWindowsInstallSession) {
        self.config = config
        self.library = library
        _session = ObservedObject(wrappedValue: session)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("\(config.displayName) — Windows 설치")
                    .font(.title2.bold())
                requestCard
                HvfWindowsInstallStatusCard(session: session)
                HvfWindowsInstallControls(session: session)
                logCard
            }
            .padding(20)
        }
        .accessibilityIdentifier("bridgevm.windows.install.view")
    }

    private var requestCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("설치 구성").font(.headline)
            row("ISO", (session.plan.request.isoPath as NSString).lastPathComponent)
            row("디스크", "\(session.plan.request.diskGiB) GiB")
            row("3D 드라이버", session.plan.request.injectViogpu3d
                ? "차단됨 — 서명 provenance 검증기 없음" : "주입 안 함")
            if session.plan.sourceImageCacheCandidateExists {
                row("설치 소스", "저장된 소스 있음 · 시작 시 확인")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color.gray.opacity(0.08))
        .cornerRadius(10)
    }

    private var logCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("로그").font(.headline)
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
                .onChange(of: session.logLines) { _, lines in
                    if !lines.isEmpty { proxy.scrollTo(lines.count - 1, anchor: .bottom) }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color.gray.opacity(0.08))
        .cornerRadius(10)
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).frame(width: 92, alignment: .leading).foregroundColor(.secondary)
            Text(value).lineLimit(1).truncationMode(.middle)
        }
        .font(.callout)
    }

}

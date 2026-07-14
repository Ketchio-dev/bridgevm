import SwiftUI

/// Detail panel for a Windows HVF VM whose unattended install has not run
/// yet: shows the persisted install request, drives the scripted-install
/// pipeline, and flips the VM to bootable when the media lands in the bundle.
struct HvfWindowsInstallView: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel
    @StateObject private var session: HvfWindowsInstallSession

    init(config: VMConfig, library: LibraryModel) {
        self.config = config
        self.library = library
        let request = HvfWindowsInstallRequest.load(bundlePath: config.bundlePath)
            ?? HvfWindowsInstallRequest(isoPath: "", diskGiB: 64,
                                        injectViogpu3d: false, driverPackageDir: nil)
        let plan = HvfWindowsInstallPlan(
            repoRoot: HvfEngineSession.defaultRepoRoot(),
            bundlePath: config.bundlePath,
            slug: config.slug,
            request: request
        )
        _session = StateObject(wrappedValue: HvfWindowsInstallSession(plan: plan))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("\(config.displayName) — Windows 설치")
                    .font(.title2.bold())
                requestCard
                stageCard
                controlRow
                logCard
            }
            .padding(20)
        }
        .onAppear { wireCompletion() }
    }

    private var requestCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("설치 구성").font(.headline)
            row("ISO", (session.plan.request.isoPath as NSString).lastPathComponent)
            row("디스크", "\(session.plan.request.diskGiB) GiB")
            row("3D 드라이버", session.plan.request.injectViogpu3d
                ? "설치 후 viogpu3d 자동 주입" : "주입 안 함")
            if session.plan.sourceImageIsCached {
                row("설치 소스", "캐시 재사용")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color.gray.opacity(0.08))
        .cornerRadius(10)
    }

    private var stageCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("진행 상태").font(.headline)
            HStack(spacing: 8) {
                if session.isRunning { ProgressView().controlSize(.small) }
                Text(session.stage.label)
                    .foregroundColor(stageColor)
                if case let .failed(message) = session.stage {
                    Text(message).font(.caption).foregroundColor(.red)
                }
            }
            if let startedAt = session.startedAt, session.isRunning {
                Text("경과: \(Int(Date().timeIntervalSince(startedAt) / 60))분 — 무인 설치는 보통 10~20분 걸립니다.")
                    .font(.caption).foregroundColor(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color.gray.opacity(0.08))
        .cornerRadius(10)
    }

    private var stageColor: Color {
        switch session.stage {
        case .done: return .green
        case .failed: return .red
        default: return .primary
        }
    }

    private var controlRow: some View {
        HStack {
            Button(session.isRunning ? "설치 진행 중…" : "설치 시작") { session.start() }
                .disabled(session.isRunning || session.stage == .done)
            if session.isRunning {
                Button("취소") { session.cancel() }
            }
            Spacer()
        }
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
                                .id(index)
                        }
                    }
                }
                .frame(height: 220)
                .onChange(of: session.logLines.count) { count in
                    if count > 0 { proxy.scrollTo(count - 1, anchor: .bottom) }
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

    private func wireCompletion() {
        session.onCompleted = { [weak library] in
            var updated = config
            updated.installPending = false
            _ = VMLibrary.save(updated)
            library?.reload()
        }
    }
}

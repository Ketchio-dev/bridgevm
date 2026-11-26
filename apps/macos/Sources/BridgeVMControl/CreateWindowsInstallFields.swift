import SwiftUI

struct CreateWindowsInstallFields: View {
    let isoPath: String
    let drivers: CreateVMDriverSelection
    let pickISO: () -> Void
    let pickGuestPayload: () -> Void
    let pickGuestPayloadManifest: () -> Void
    let fetchDrivers: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 8) {
                Text("Windows 11 ARM64 설치 이미지").font(.headline)
                fileRow("ISO 선택…", path: isoPath, id: "iso", action: pickISO)
            }

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("설치 드라이버").font(.headline)
                    Spacer()
                    Button("드라이버 자동 준비", action: fetchDrivers)
                        .disabled(drivers.fetchInFlight)
                        .accessibilityIdentifier("bridgevm.create.windows.guest-payload.fetch")
                }
                driverStatus.font(.callout)
                fileRow("ARM64 드라이버 폴더…", path: drivers.payloadPath,
                        id: "guest-payload", action: pickGuestPayload)
                fileRow("SHA-256 목록 (.tsv)…", path: drivers.manifestPath,
                        id: "guest-manifest", action: pickGuestPayloadManifest)
            }
            Text("Windows는 3D 가속 없이 기본 디스플레이로 설치됩니다.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private var driverStatus: some View {
        switch drivers.status {
        case .idle:
            Text("자동 준비를 선택하면 Microsoft 서명 virtio-win 드라이버(약 95 MB)를 내려받아 확인합니다.")
                .foregroundStyle(.secondary)
            Text("직접 준비한 드라이버가 있다면 폴더와 SHA-256 목록을 선택하세요.")
        case .preparing:
            HStack {
                ProgressView().controlSize(.small)
                Text("공식 드라이버를 준비하는 중…")
            }
        case .prepared:
            Label("공식 드라이버와 SHA-256 목록을 준비했습니다.", systemImage: "checkmark.circle")
        case .manual:
            Text(drivers.ready ? "직접 선택한 드라이버와 SHA-256 목록을 사용합니다."
                               : "드라이버 폴더와 SHA-256 목록을 모두 선택하세요.")
        case .failed(let message):
            Text(message).foregroundStyle(.red)
            Text(drivers.ready ? "아래에 표시된 드라이버와 SHA-256 목록으로 생성할 수 있습니다."
                               : "드라이버 폴더와 SHA-256 목록을 직접 선택할 수도 있습니다.")
        }
    }

    private func fileRow(_ title: String, path: String, id: String, action: @escaping () -> Void) -> some View {
        let identifier = "bridgevm.create.windows.\(id)"
        return HStack(spacing: 8) {
            Button(title, action: action).accessibilityIdentifier(identifier)
            Text(path.isEmpty ? "선택 안 함" : (path as NSString).lastPathComponent)
                .font(.callout).foregroundStyle(.secondary)
                .lineLimit(1).truncationMode(.middle)
                .accessibilityIdentifier("\(identifier).selection")
                .accessibilityValue(path)
        }
    }
}

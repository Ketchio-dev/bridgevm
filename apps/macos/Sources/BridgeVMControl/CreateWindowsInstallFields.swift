import SwiftUI

struct CreateWindowsInstallFields: View {
    let isoPath: String
    let guestPayloadPath: String
    let guestPayloadManifestPath: String
    let pickISO: () -> Void
    let pickGuestPayload: () -> Void
    let pickGuestPayloadManifest: () -> Void
    var fetchingDrivers = false
    var driverFetchError = ""
    var fetchDrivers: () -> Void = {}

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
                        .disabled(fetchingDrivers)
                        .accessibilityIdentifier("bridgevm.create.windows.guest-payload.fetch")
                }
                if fetchingDrivers {
                    HStack {
                        ProgressView().controlSize(.small)
                        Text("공식 드라이버를 내려받는 중…").font(.callout)
                    }
                } else {
                    Text("자동 준비를 선택하면 Microsoft 서명 virtio-win 드라이버(약 95 MB)를 내려받아 확인합니다.")
                        .font(.callout).foregroundStyle(.secondary)
                }
                if !driverFetchError.isEmpty {
                    Text(driverFetchError).font(.callout).foregroundStyle(.red)
                }
                Text("직접 준비한 드라이버가 있다면 폴더와 SHA-256 목록을 선택하세요.")
                    .font(.callout).padding(.top, 4)
                fileRow("ARM64 드라이버 폴더…", path: guestPayloadPath,
                        id: "guest-payload", action: pickGuestPayload)
                fileRow("SHA-256 목록 (.tsv)…", path: guestPayloadManifestPath,
                        id: "guest-manifest", action: pickGuestPayloadManifest)
            }
            Text("Windows는 3D 가속 없이 기본 디스플레이로 설치됩니다.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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

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
        VStack(alignment: .leading, spacing: 12) {
            Text("Windows 11 ARM64 ISO에서 자체 HVF 엔진으로 무인 설치합니다.")
                .font(.callout).foregroundColor(.secondary)
            GroupBox {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Button("ISO 선택…", action: pickISO)
                            .accessibilityIdentifier("bridgevm.create.windows.iso")
                        Text(isoPath.isEmpty ? "선택된 ISO 없음" : (isoPath as NSString).lastPathComponent)
                            .font(.caption).foregroundColor(.secondary).lineLimit(1)
                            .accessibilityIdentifier("bridgevm.create.windows.iso.selection").accessibilityValue(isoPath)
                    }
                    HStack {
                        Button("드라이버 자동 준비", action: fetchDrivers)
                            .disabled(fetchingDrivers)
                            .accessibilityIdentifier("bridgevm.create.windows.guest-payload.fetch")
                        if fetchingDrivers {
                            ProgressView().controlSize(.small)
                            Text("공식 드라이버를 내려받는 중…").font(.caption).foregroundColor(.secondary)
                        }
                    }
                    if !driverFetchError.isEmpty {
                        Text(driverFetchError).font(.caption).foregroundColor(.red)
                    }
                    HStack {
                        Button("ARM64 드라이버 폴더…", action: pickGuestPayload)
                            .accessibilityIdentifier("bridgevm.create.windows.guest-payload")
                        Text(guestPayloadPath.isEmpty ? "선택된 payload 없음" : (guestPayloadPath as NSString).lastPathComponent)
                            .font(.caption).foregroundColor(.secondary).lineLimit(1)
                            .accessibilityIdentifier("bridgevm.create.windows.guest-payload.selection").accessibilityValue(guestPayloadPath)
                    }
                    HStack {
                        Button("Payload manifest…", action: pickGuestPayloadManifest)
                            .accessibilityIdentifier("bridgevm.create.windows.guest-manifest")
                        Text(guestPayloadManifestPath.isEmpty ? "선택된 manifest 없음" : (guestPayloadManifestPath as NSString).lastPathComponent)
                            .font(.caption).foregroundColor(.secondary).lineLimit(1)
                            .accessibilityIdentifier("bridgevm.create.windows.guest-manifest.selection").accessibilityValue(guestPayloadManifestPath)
                    }
                    Text("ISO만 있으면 됩니다. '드라이버 자동 준비'를 누르면 Microsoft 서명된 Red Hat virtio-win 저장장치·직렬·네트워크 드라이버(약 95 MB)를 공식 배포처에서 내려받아 확인합니다. 직접 준비한 ARM64 드라이버 폴더와 SHA-256 manifest를 골라도 됩니다. 드라이버는 VM 번들에 복사·봉인됩니다.")
                        .font(.caption).foregroundColor(.secondary)
                }
                .padding(6)
            } label: {
                Label("설치 파일", systemImage: "opticaldisc")
            }
            Text("이번 버전은 3D 가속을 지원하지 않습니다. Windows는 기본 디스플레이로 설치합니다.")
                .font(.caption).foregroundColor(.secondary)
        }
    }
}

import SwiftUI

struct CreateWindowsInstallFields: View {
    let isoPath: String
    let guestPayloadPath: String
    let guestPayloadManifestPath: String
    let pickISO: () -> Void
    let pickGuestPayload: () -> Void
    let pickGuestPayloadManifest: () -> Void

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
                    Text("저장장치·직렬·네트워크용 서명된 ARM64 드라이버와 SHA-256 manifest가 필요합니다. 선택한 원본은 VM 번들에 복사·봉인됩니다.")
                        .font(.caption).foregroundColor(.secondary)
                }
                .padding(6)
            } label: {
                Label("설치 파일", systemImage: "opticaldisc")
            }
            Text("3D 드라이버 주입은 서명 provenance 검증기가 없어 사용할 수 없습니다. Windows는 3D 주입 없이 설치합니다.")
                .font(.caption).foregroundColor(.secondary)
        }
    }
}

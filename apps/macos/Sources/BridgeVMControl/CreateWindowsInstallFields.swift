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
                Text("Windows 11 Arm installer").font(.headline)
                fileRow("Choose ISO…", path: isoPath, id: "iso", action: pickISO)
            }

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Windows setup drivers").font(.headline)
                    Spacer()
                    Button("Download Drivers", action: fetchDrivers)
                        .disabled(drivers.fetchInFlight)
                        .accessibilityIdentifier("bridgevm.create.windows.guest-payload.fetch")
                }
                driverStatus.font(.callout)
                fileRow("Driver Folder…", path: drivers.payloadPath,
                        id: "guest-payload", action: pickGuestPayload)
                fileRow("SHA-256 Manifest…", path: drivers.manifestPath,
                        id: "guest-manifest", action: pickGuestPayloadManifest)
            }
            Text("Windows uses the standard display with 3D acceleration off.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private var driverStatus: some View {
        switch drivers.status {
        case .idle:
            Text("Download Drivers fetches and verifies the Microsoft-signed virtio-win drivers (about 95 MB).")
                .foregroundStyle(.secondary)
            Text("If you already have the Arm64 drivers, choose their folder and SHA-256 manifest (.tsv).")
        case .preparing:
            HStack {
                ProgressView().controlSize(.small)
                Text(drivers.downloadPercent.map { "Downloading the drivers… \($0)%" } ?? "Preparing the drivers…")
            }
        case .prepared:
            Label("Drivers and SHA-256 manifest are ready.", systemImage: "checkmark.circle")
        case .manual:
            Text(drivers.ready ? "Using your driver folder and SHA-256 manifest."
                               : "Choose both the driver folder and its SHA-256 manifest.")
        case .failed(let message):
            Text(message).foregroundStyle(.red)
            Text(drivers.ready ? "You can continue with the drivers and manifest shown below."
                               : "You can also choose a driver folder and its SHA-256 manifest.")
        }
    }

    private func fileRow(_ title: String, path: String, id: String, action: @escaping () -> Void) -> some View {
        let identifier = "bridgevm.create.windows.\(id)"
        return HStack(spacing: 8) {
            Button(title, action: action).accessibilityIdentifier(identifier)
            Text(path.isEmpty ? "None selected" : (path as NSString).lastPathComponent)
                .font(.callout).foregroundStyle(.secondary)
                .lineLimit(1).truncationMode(.middle)
                .accessibilityIdentifier("\(identifier).selection")
                .accessibilityValue(path)
        }
    }
}

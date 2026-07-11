import SwiftUI

struct SettingsView: View {
    @ObservedObject var settings: AppSettings
    @State private var windowsHVFLabMessage: String?
    @State private var windowsHVFLabFailed = false
    var storeDoctorState: BridgeVMAppModel.StoreDoctorState = .idle
    var bundledDaemonLaunchReport: BundledDaemonLaunchReport?
    var onApply: () -> Void
    var onCheckStoreDoctor: () -> Void = {}

    var body: some View {
        Form {
            Section("Daemon") {
                Toggle("Use mock inventory", isOn: $settings.useMockInventory)

                Toggle("Allow Apple VZ live starts", isOn: $settings.allowAppleVzRealStart)
                    .disabled(settings.useMockInventory)
                    .help("Pass BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 to the bundled daemon")

                TextField("Socket path", text: $settings.daemonSocketPath)
                    .disabled(settings.useMockInventory)

                Label(settings.daemonModeSummary, systemImage: settings.useMockInventory ? "shippingbox" : "server.rack")
                    .font(.callout)
                    .foregroundStyle(settings.hasValidDaemonSettings ? Color.secondary : Color.red)
                    .fixedSize(horizontal: false, vertical: true)

                if let bundledDaemonLaunchReport {
                    let diagnostics = BundledDaemonDiagnosticsSummary(
                        report: bundledDaemonLaunchReport
                    )
                    Label(
                        diagnostics.statusText,
                        systemImage: diagnostics.isHealthy
                            ? "checkmark.circle"
                            : "exclamationmark.triangle"
                    )
                    .font(.callout)
                    .foregroundStyle(diagnostics.isHealthy ? Color.secondary : Color.red)
                    .fixedSize(horizontal: false, vertical: true)

                    if let helperPath = diagnostics.helperPath {
                        StoreDoctorPathRow(title: "Helper", path: helperPath)
                    }

                    if let socketPath = diagnostics.socketPath {
                        StoreDoctorPathRow(title: "Socket", path: socketPath)
                    }

                    if let stderrPreview = diagnostics.stderrPreview {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Daemon output")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text(stderrPreview)
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                                .lineLimit(4)
                        }
                    }
                }

                HStack {
                    Button("Reset") {
                        settings.resetDaemonSocketPath()
                    }
                    .disabled(settings.useMockInventory)

                    Spacer()

                    Button("Apply") {
                        onApply()
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(!settings.hasPendingChanges || !settings.hasValidDaemonSettings)
                    .help(settingsApplyHelp)
                }
            }

            Section("Store Doctor") {
                Label(storeDoctorSummary, systemImage: storeDoctorIcon)
                    .font(.callout)
                    .foregroundStyle(storeDoctorColor)
                    .fixedSize(horizontal: false, vertical: true)

                if case let .ready(report) = storeDoctorState {
                    VStack(alignment: .leading, spacing: 4) {
                        StoreDoctorPathRow(title: "Store", path: report.storeRoot)
                        StoreDoctorPathRow(title: "VMs", path: report.vmsDir)
                    }
                }

                Button {
                    onCheckStoreDoctor()
                } label: {
                    if storeDoctorState == .checking {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text("Run Health Check")
                    }
                }
                .disabled(storeDoctorState == .checking || !settings.hasValidDaemonSettings)
                .help("Ask the daemon for store metadata without launching a VM")
            }

            Section("Windows on HVF") {
                Text("Import a proven installed Windows ARM RAW disk and its matching UEFI variables into the bundled experimental HVF runtime.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                Button("Open Windows HVF Lab") {
                    do {
                        try WindowsHVFLabLauncher.open()
                        windowsHVFLabMessage = "Windows HVF Lab opened."
                        windowsHVFLabFailed = false
                    } catch {
                        windowsHVFLabMessage = error.localizedDescription
                        windowsHVFLabFailed = true
                    }
                }
                .help("Open the separately isolated Windows HVF control surface bundled inside BridgeVM")

                if let windowsHVFLabMessage {
                    Label(
                        windowsHVFLabMessage,
                        systemImage: windowsHVFLabFailed ? "exclamationmark.triangle" : "checkmark.circle"
                    )
                    .font(.caption)
                    .foregroundStyle(windowsHVFLabFailed ? Color.red : Color.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .formStyle(.grouped)
        .padding(20)
        .frame(width: 420)
    }

    private var settingsApplyHelp: String {
        if !settings.hasValidDaemonSettings {
            return "Use a daemon socket path or mock inventory"
        }
        return settings.hasPendingChanges ? "Apply settings to the dashboard" : "Settings already applied"
    }

    private var storeDoctorSummary: String {
        switch storeDoctorState {
        case .idle:
            return settings.useMockInventory
                ? "Mock inventory is enabled. Health check reports mock store readiness."
                : "Check daemon store readiness without starting or changing a VM."
        case .checking:
            return "Checking store readiness..."
        case let .ready(report):
            if settings.useMockInventory || report.status == "MOCK" {
                return "Mock store metadata is available."
            }
            return "Daemon store status: \(report.status)"
        case let .failed(message):
            return message
        }
    }

    private var storeDoctorIcon: String {
        switch storeDoctorState {
        case .idle:
            return "stethoscope"
        case .checking:
            return "hourglass"
        case let .ready(report):
            return report.isReady ? "checkmark.seal" : "shippingbox"
        case .failed:
            return "exclamationmark.triangle"
        }
    }

    private var storeDoctorColor: Color {
        switch storeDoctorState {
        case .failed:
            return .red
        case let .ready(report):
            return report.isReady ? .green : .secondary
        default:
            return .secondary
        }
    }
}

struct BundledDaemonDiagnosticsSummary: Equatable {
    var statusText: String
    var isHealthy: Bool
    var helperPath: String?
    var socketPath: String?
    var stderrPreview: String?

    init(report: BundledDaemonLaunchReport) {
        statusText = report.detail
        isHealthy = report.isHealthy
        helperPath = report.helperPath
        socketPath = report.socketPath
        stderrPreview = report.stderrTail?.trimmingCharacters(in: .whitespacesAndNewlines)
        if stderrPreview?.isEmpty == true {
            stderrPreview = nil
        }
    }
}

private struct StoreDoctorPathRow: View {
    var title: String
    var path: String

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .foregroundStyle(.secondary)
                .frame(width: 42, alignment: .leading)
            Text(path)
                .font(.caption.monospaced())
                .textSelection(.enabled)
                .lineLimit(2)
        }
    }
}

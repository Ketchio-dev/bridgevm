import SwiftUI

struct VMRow: View {
    let config: VMConfig

    var body: some View {
        HStack(spacing: 10) {
            LibraryVMEmblem(engine: config.engineKind, compact: true)
            VStack(alignment: .leading, spacing: 5) {
                Text(config.name).font(.body.weight(.semibold)).lineLimit(1)
                Text(config.engineShortLabel + " · " + (config.installPending == true ? "설치 필요" : "등록됨"))
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 7)
        .help(config.name)
        .accessibilityElement(children: .combine)
    }
}

struct LibraryVMCard: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                library.proMode = false
                library.selectedID = config.slug
            } label: {
                VStack(alignment: .leading, spacing: 18) {
                    HStack(alignment: .top) {
                        LibraryVMEmblem(engine: config.engineKind)
                        Spacer()
                        Text(config.engineShortLabel)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 10).padding(.vertical, 6)
                            .background(.primary.opacity(0.05), in: Capsule())
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text(config.name).font(.title3.weight(.semibold)).lineLimit(2)
                            .frame(height: 48, alignment: .topLeading)
                        Text("저장된 구성").font(.caption).foregroundStyle(.secondary)
                        Text(resourceDescription).font(.callout).foregroundStyle(.secondary)
                            .lineLimit(1).minimumScaleFactor(0.8)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(20)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain).help(config.name)
            .accessibilityLabel("\(config.name), \(config.engineShortLabel), \(resourceDescription)")
            .accessibilityHint("VM 상세 화면 열기")
            Divider().padding(.horizontal, 20)
            HStack {
                Label(config.installPending == true ? "설치 필요" : "등록됨",
                      systemImage: config.installPending == true ? "arrow.down.circle" : "folder")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                Menu {
                    LibraryVMContextMenu(library: library, config: config)
                } label: {
                    Image(systemName: "ellipsis").frame(width: 28, height: 24)
                }
                .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
                .accessibilityLabel("\(config.name) 작업")
                .help("VM 작업")
            }
            .padding(.horizontal, 20).padding(.vertical, 10)
        }
        .modifier(LibraryCardSurface())
        .contextMenu { LibraryVMContextMenu(library: library, config: config) }
    }

    private var resourceDescription: String {
        var parts = [String]()
        if let cpus = config.cpuCount, cpus > 0 { parts.append("\(cpus) vCPU") }
        if let memory = config.memMiB, memory > 0 {
            parts.append(memory.isMultiple(of: 1024) ? "\(memory / 1024) GiB RAM" : "\(memory) MiB RAM")
        }
        return parts.isEmpty ? "상세 화면에서 설정 확인" : parts.joined(separator: "  ·  ")
    }
}

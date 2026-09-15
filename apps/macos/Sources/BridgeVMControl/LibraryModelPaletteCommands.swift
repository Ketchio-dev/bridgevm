extension LibraryModel {
    func selectedPaletteCommands(dismiss: @escaping () -> Void) -> [PaletteCommand] {
        if selectedID == Self.hvfEngineSelectionID {
            return [paletteControlNavigation(id: Self.hvfEngineSelectionID, name: "HVF Engine", dismiss: dismiss)]
        }
        guard let id = selectedID, let config = vms.first(where: { $0.slug == id }) else { return [] }
        if config.engineKind == .hvfEngine || windowsInstallSessions.isActive(slug: id)
            || hvfRuntimeSessions.isActive(slug: id) {
            return [paletteControlNavigation(id: id, name: config.name, dismiss: dismiss)]
        }
        var c: [PaletteCommand] = []
        if let m = selectedModel {
            if m.config.engineKind == .hvfEngine {
                return [paletteControlNavigation(id: id, name: m.config.name, dismiss: dismiss)]
            }
            if !m.running && !m.lifecycleBusy {
                c.append(.init(title: "시작: \(m.config.name)", subtitle: "VM 시작 / 창 열기", systemImage: "play.fill") { m.start(); dismiss() })
            } else if m.running && !m.lifecycleBusy {
                c.append(.init(title: "정지: \(m.config.name)", subtitle: "VM 정지", systemImage: "stop.fill") { m.stop(); dismiss() })
            }
            c.append(.init(title: "새로고침: \(m.config.name)", subtitle: "상태 갱신", systemImage: "arrow.clockwise") { m.refresh(); dismiss() })
        }
        return c
    }

    private func paletteControlNavigation(id: String, name: String, dismiss: @escaping () -> Void) -> PaletteCommand {
        .init(title: "VM 제어 화면 열기: \(name)", subtitle: "시작·중지와 설치 상태 확인", systemImage: "desktopcomputer") {
            self.selectedID = id
            self.proMode = false
            dismiss()
        }
    }
}

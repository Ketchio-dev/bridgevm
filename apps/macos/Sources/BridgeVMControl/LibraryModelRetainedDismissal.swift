extension LibraryModel {
    /// Closing a terminal generic record also resolves its slot through the ordinary lookup,
    /// so admission and file actions stop seeing the retained control at once.
    @discardableResult
    func dismissRetainedControl(_ token: String) -> Bool {
        let record = retainedControlRecords.first { $0.id == token }
        guard retainedControlStore.dismiss(token) else { return false }
        if case let .generic(config, _)? = record?.descriptor, let current = vms.first(where: { $0.slug == config.slug }) {
            _ = model(for: current)
        }
        if selectedID == token { selectedID = vms.first?.slug }
        return true
    }

    /// Delete, clone and move refuse while a stale control holds the slot. When that control
    /// is retained with an open record, closing the record is what releases the slot.
    func mismatchedCachedControlRefusal(for cfg: VMConfig) -> String {
        guard let cached = cachedControlModels[cfg.slug], cached.isRetained,
              retainedControlRecords.contains(where: { $0.descriptor.identity == .generic(ObjectIdentifier(cached)) })
        else { return "VM 제어 정보가 최신 등록 정보와 다릅니다. 기존 VM을 완전히 중지하고 목록을 새로고침한 뒤 다시 시도하세요." }
        return "이전 등록 정보로 보관한 제어가 남아 있습니다. 사이드바 '진행 중이거나 종료된 작업'에서 그 항목을 열어, 실행이 끝났으면 '목록에서 닫기'를 누른 뒤 다시 시도하세요."
    }
}

typealias LibraryWorkAdmission = @MainActor (_ reportRefusal: Bool) -> String?

extension LibraryModel {
    func deletionImpact(for cfg: VMConfig) -> VMLibraryDeletionImpact {
        VMLibrary.deletionImpact(for: cfg, rootURL: rootURL)
    }

}

extension LibraryModel {
    func latestConfiguration(for config: VMConfig) -> VMConfig {
        vms.first(where: { $0.slug == config.slug }) ?? config
    }

    func boundWorkAdmission(
        slug: String,
        owns: @escaping @MainActor (LibraryModel, VMConfig) -> Bool
    ) -> LibraryWorkAdmission {
        { [weak self] reportRefusal in
            guard let self else {
                return "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."
            }
            let refusal = self.libraryWorkRefusal(slug: slug, owns: owns)
            if reportRefusal, let refusal { self.operationError = refusal }
            return refusal
        }
    }

}
